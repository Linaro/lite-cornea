use std::borrow::Borrow;
use std::collections::hash_map::{Entry, HashMap};
use std::convert::TryInto;

use gdbstub::arch::{Arch, RegId, Registers};
use gdbstub::outputln;
use gdbstub::target::ext::base::singlethread::{SingleThreadOps, StopReason};
use gdbstub::target::ext::base::{BaseOps, ResumeAction};
#[allow(unused)]
use gdbstub::target::ext::breakpoints::{
    Breakpoints, BreakpointsOps, HwBreakpoint, HwBreakpointOps, SwBreakpoint, SwBreakpointOps,
};
use gdbstub::target::ext::monitor_cmd::{ConsoleOutput, MonitorCmd, MonitorCmdOps};
use gdbstub::target::{Target, TargetResult};

use crate::{
    breakpoint, instance_registry, memory, resource, simulation, simulation_time, step,
    FastModelIris,
};

pub struct IrisGdbStub<'i> {
    pub iris: &'i mut FastModelIris,
    pub instance_id: u32,
    sim: u32,
    breakpoints: HashMap<u64, Vec<u64>>,
    resources: Option<Vec<resource::ResourceInfo>>,
    spaces: Option<Vec<memory::Space>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuestState {
    pub regs: Vec<u64>,
}

impl Default for GuestState {
    fn default() -> Self {
        Self { regs: vec![0; 98] }
    }
}

impl<'i> IrisGdbStub<'i> {
    pub fn from_instance(iris: &'i mut FastModelIris, instance_id: u32) -> std::io::Result<Self> {
        let sim = instance_registry::get_instance_by_name(
            iris,
            "framework.SimulationEngine".to_string(),
        )?;
        Ok(Self {
            iris,
            instance_id,
            breakpoints: HashMap::new(),
            sim: sim.id,
            resources: None,
            spaces: None,
        })
    }
}

impl Registers for GuestState {
    type ProgramCounter = u64;
    fn pc(&self) -> u64 {
        self.regs[32]
    }
    fn gdb_serialize(&self, mut write_byte: impl FnMut(Option<u8>)) {
        for reg in &self.regs {
            for byte in reg.to_le_bytes().iter() {
                write_byte(Some(*byte));
            }
        }
        write_byte(Some(0));
        write_byte(Some(0));
        write_byte(Some(0));
        write_byte(Some(0));
    }
    fn gdb_deserialize(&mut self, bytes: &[u8]) -> Result<(), ()> {
        if bytes.len() % 8 != 0 {
            return Err(());
        }
        let mut regs = bytes
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes(c.try_into().unwrap()));
        for reg in &mut self.regs {
            *reg = regs.next().ok_or(())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Register {
    X(u8),
    SP,
    PC,
    XPSR,
}

impl RegId for Register {
    fn from_raw_id(id: usize) -> Option<(Self, usize)> {
        use Register::*;
        Some(match id {
            32 => PC,
            33 => XPSR,
            31 => SP,
            id if id < 31 => X(id as u8),
            _ => return None,
        })
        .map(|r| (r, 0))
    }
}

impl<'i> Target for IrisGdbStub<'i> {
    type Arch = Armv8aArch;
    type Error = ();
    fn base_ops(&mut self) -> BaseOps<'_, Self::Arch, Self::Error> {
        BaseOps::SingleThread(self)
    }

    fn breakpoints(&mut self) -> Option<BreakpointsOps<Self>> {
        Some(self)
    }

    fn monitor_cmd(&mut self) -> Option<MonitorCmdOps<Self>> {
        Some(self)
    }
}

impl SingleThreadOps for IrisGdbStub<'_> {
    fn read_registers(&mut self, regs: &mut GuestState) -> TargetResult<(), Self> {
        if self.resources.is_none() {
                let resources = resource::get_list(&mut self.iris, self.instance_id, None, None).map_err(|_| ())?;
                self.resources = Some(resources);
        };
        for res in self.resources.as_ref().unwrap() {
            let regnum = match res.name.as_str() {
                "PC" => 32,
                "SP" => 31,
                "XPSR" => 33,
                "CPSR" => 33,
                x if x.starts_with("X") => {
                    if let Ok(regnum) = x[1..].parse() {
                        regnum
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };
            let val =
                resource::read(&mut self.iris, self.instance_id, vec![res.id]).map_err(|_| ())?;
            if !val.data.is_empty() {
                regs.regs[regnum] = val.data[0]
            }
        }
        Ok(())
    }

    fn read_addrs(&mut self, start_addr: u64, data: &mut [u8]) -> TargetResult<(), Self> {
        let mem = memory::read(
            &mut self.iris,
            self.instance_id,
            0,
            start_addr as u64,
            1,
            data.len() as u64,
        )
        .map_err(|_| ())?;
        for (offset, byte) in mem
            .data
            .into_iter()
            .map(|u| u.to_le_bytes())
            .flatten()
            .enumerate()
        {
            if data.len() > offset {
                data[offset] = byte;
            }
        }
        Ok(())
    }

    fn write_addrs(&mut self, _: u64, _: &[u8]) -> TargetResult<(), Self> {
        Ok(())
    }
    fn write_registers(&mut self, _: &GuestState) -> TargetResult<(), Self> {
        // We don't support writing
        Ok(())
    }

    fn resume(
        &mut self,
        act: ResumeAction,
        intr: gdbstub::target::ext::base::GdbInterrupt<'_>,
    ) -> Result<StopReason<u64>, ()> {
        let mut interrupt = intr.no_async();
        if act == ResumeAction::Step {
            step::setup(self.iris, self.instance_id, 1, step::Unit::Instruction).map_err(|_| ())?
        }
        if act == ResumeAction::Step || act == ResumeAction::Continue {
            simulation_time::run(self.iris, self.sim).map_err(|_| ())?;
            while simulation_time::get(self.iris, self.sim)
                .map_err(|_| ())?
                .running
            {
                if interrupt.pending() {
                    simulation_time::stop(self.iris, self.sim).map_err(|_| ())?;
                    return Ok(StopReason::GdbInterrupt);
                }
            }
            if act == ResumeAction::Step {
                return Ok(StopReason::DoneStep);
            } else {
                return Ok(StopReason::HwBreak);
            }
        }
        Err(())
    }
}

impl<'i> Breakpoints for IrisGdbStub<'i> {
    fn hw_breakpoint(&mut self) -> Option<HwBreakpointOps<Self>> {
        Some(self)
    }

    fn sw_breakpoint(&mut self) -> Option<SwBreakpointOps<Self>> {
        Some(self)
    }
}
impl<'i> SwBreakpoint for IrisGdbStub<'i> {
    fn add_sw_breakpoint(
        &mut self,
        addr: <Self::Arch as Arch>::Usize,
        k: <Self::Arch as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        self.add_hw_breakpoint(addr, k)
    }

    fn remove_sw_breakpoint(
        &mut self,
        addr: <Self::Arch as Arch>::Usize,
        k: <Self::Arch as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        self.remove_hw_breakpoint(addr, k)
    }
}

impl<'i> HwBreakpoint for IrisGdbStub<'i> {
    fn add_hw_breakpoint(
        &mut self,
        addr: <Self::Arch as Arch>::Usize,
        _: <Self::Arch as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        if self.breakpoints.contains_key(&addr) {
            return Ok(true);
        }
        if self.spaces.is_none() {
                let spaces= memory::spaces(self.iris, self.instance_id)?;
                self.spaces = Some(spaces);
        };
        let Self { spaces, iris, instance_id, .. } = self;
        let store: Vec<u64> = spaces.as_ref().unwrap().iter().filter_map(|space| {
            breakpoint::code(
                iris,
                *instance_id,
                addr as u64,
                None,
                space.id,
                true,
                false,
            ).ok()
        }).collect();

        if store.is_empty() {
            Ok(false)
        } else {
            self.breakpoints.insert(addr, store);
            Ok(true)
        }
    }
    fn remove_hw_breakpoint(
        &mut self,
        addr: <Self::Arch as Arch>::Usize,
        _: <Self::Arch as Arch>::BreakpointKind,
    ) -> TargetResult<bool, Self> {
        if let Entry::Occupied(ent) = self.breakpoints.entry(addr) {
            for bkpt in ent.get() {
                if let Err(_) = breakpoint::delete(self.iris, self.instance_id, *bkpt) {
                    return Ok(false)
                }
            }
            let _ = ent.remove_entry();
        }
        Ok(true)
    }
}

impl<'i> MonitorCmd for IrisGdbStub<'i> {
    fn handle_monitor_cmd(&mut self, cmd: &[u8], mut out: ConsoleOutput<'_>) -> Result<(), ()> {
        match String::from_utf8_lossy(cmd).borrow() {
            "reset" => {
                simulation::reset(self.iris, self.sim, false).map_err(|_| ())?;
                simulation::wait(self.iris, self.sim).map_err(|_| ())?;
            }
            c => {
                outputln!(out, "Monitor command {} not supported", c);
            }
        }
        Ok(())
    }
}

pub enum Armv8aArch {}
impl Arch for Armv8aArch {
    type Usize = u64;
    type Registers = GuestState;
    type RegId = Register;
    type BreakpointKind = usize;
}

pub use crate::gdb::t32::GdbOverPipe;
