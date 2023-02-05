use std::convert::TryInto;
use std::io::{stdin, stdout};
use std::str::FromStr;

use clap::{Parser, Subcommand};
use gdbstub::GdbStub;

#[allow(unused)]
use cornea::{
    breakpoint, checkpoint, event, event_stream, instance_registry, memory, resource, simulation,
    simulation_time, step, FastModelIris,
};

#[derive(Parser, Debug)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
    #[clap(short, long)]
    port: Option<u16>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print information about an instance
    ResourceList(InstanceArgs),
    /// Print the children of this instance
    ChildList(OptionalInstanceArgs),
    /// Read memory from the prespective of an instance
    MemoryRead(ReadMemArgs),
    /// Break at a pc range
    Break(ReadMemArgs),
    /// Reset the platform
    Reset,
    /// Read a reesource on an instance
    ResourceRead(ResourceReadArgs),
    /// Provide a GDB server for the iris server over a pipe
    GdbProxy(InstanceArgs),
}

#[derive(Parser, Debug)]
struct OptionalInstanceArgs {
    /// The name of the instance to query
    inst: Option<String>,
}

#[derive(Parser, Debug)]
struct InstanceArgs {
    /// The name of the instance to query
    inst: String,
}

#[derive(Parser, Debug)]
struct ReadMemArgs {
    /// The name of the instance to read from
    inst: String,
    /// Address to print from
    addr: String,
    /// Size of memory block to print in bytes. When not present defaults to
    /// 4 bytes
    size: Option<String>,
    /// Type of the memory block
    #[clap(short, long)]
    group_by: Option<GroupBy>,
}

#[derive(Parser, Debug)]
struct ResourceReadArgs {
    /// The name of the instance to read from
    inst: String,
    /// Resource to print from
    resource: String,
}

#[allow(unused)]
fn enable_events(
    fvp: &mut FastModelIris,
    my_id: u32,
    cpus: &[instance_registry::Instance],
    event_names: &[&str],
) -> std::io::Result<()> {
    for cpu in cpus {
        let sources = event_names
            .iter()
            .map(|name| event::Source {
                id: cpu.id,
                name: name.to_string(),
            })
            .collect::<Vec<_>>();
        let sources = fvp.batch(&sources)?;
        let streams = sources
            .into_iter()
            .map(|src| event_stream::Create {
                id: Some(cpu.id),
                disable: false,
                to_id: my_id,
                source: src.id,
                buffer: false,
            })
            .collect::<Vec<_>>();
        fvp.batch(&streams)?;
    }
    Ok(())
}

#[derive(Parser, Debug)]
enum GroupBy {
    U64,
    U32,
    U16,
    U8,
}

impl FromStr for GroupBy {
    /// TODO:  v make this a better type
    type Err = String;
    fn from_str(f: &str) -> Result<Self, String> {
        Ok(match f {
            "u8" | "char" | "uint8_t" => Self::U8,
            "u16" | "short" | "uint16_t" => Self::U16,
            "u32" | "int" | "uint32_t" => Self::U32,
            "u64" | "long" | "uint64_t" => Self::U64,
            _ => Err("".to_string())?,
        })
    }
}

fn print_hex_dump(address: u64, buff: &[u8], group_by: GroupBy) {
    match group_by {
        GroupBy::U8 => println!("         0  1  2  3  4  5  6  7  8  9  a  b  c  d  e  f"),
        GroupBy::U16 => println!("         0    2    4    6    8    a    c    e"),
        GroupBy::U32 => println!("         0        4        8        c"),
        GroupBy::U64 => println!("         0                8"),
    }
    let addr_range = (address as usize)..(address as usize + buff.len());
    let base = (address & !0xf) as usize;
    let step = match group_by {
        GroupBy::U8 => 1,
        GroupBy::U16 => 2,
        GroupBy::U32 => 4,
        GroupBy::U64 => 8,
    };
    for base_addr in (base..base + buff.len()).step_by(0x10) {
        print!("{:08x}", base_addr);
        for cur_addr in (base_addr..base_addr + 0x10).step_by(step) {
            if addr_range.contains(&cur_addr) {
                let offset = cur_addr - address as usize;
                let slice = &buff[offset..offset + step];
                match group_by {
                    GroupBy::U8 => print!(" {:02x}", buff[offset]),
                    GroupBy::U16 => {
                        print!(" {:04x}", u16::from_le_bytes(slice.try_into().unwrap()))
                    }
                    GroupBy::U32 => {
                        print!(" {:08x}", u32::from_le_bytes(slice.try_into().unwrap()))
                    }
                    GroupBy::U64 => {
                        print!(" {:016x}", u64::from_le_bytes(slice.try_into().unwrap()))
                    }
                }
            } else {
                print!(" {:width$}", "", width = step * 2);
            }
        }
        print!(" ");
        for cur_addr in base_addr..base_addr + 0x10 {
            if addr_range.contains(&cur_addr) {
                let byte = buff[cur_addr - address as usize];
                if byte.is_ascii_graphic() {
                    print!("{}", char::from(byte));
                } else {
                    print!(".");
                }
            } else {
                print!(" ");
            }
        }
        println!("");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    let mut fvp = FastModelIris::from_port(None, args.port.unwrap_or(7100))
        .or_else(|_| FastModelIris::from_port(None, args.port.unwrap_or(7101)))
        .or_else(|_| FastModelIris::from_port(None, args.port.unwrap_or(7102)))
        .or_else(|_| FastModelIris::from_port(None, args.port.unwrap_or(7103)))
        .or_else(|_| FastModelIris::from_port(None, args.port.unwrap_or(7104)))?;
    let _my_id = fvp.register()?;
    use Command::*;
    match args.command {
        ResourceList(InstanceArgs { inst }) => {
            let instance = instance_registry::get_instance_by_name(&mut fvp, inst).unwrap();
            println!(
                "{:<6}│{:^6}│ {:>20} │ {}",
                "type", "bits", "name", "description"
            );
            println!("{:═<6}╪{:═^6}╪═{:═>20}═╪═{:═<20}", "", "", "", "");
            for res in resource::get_list(&mut fvp, instance.id, None, None)? {
                let typ = if res.parameter_info.is_none() {
                    "Reg"
                } else {
                    "Param"
                };
                let name = res.name;
                let bits = res.bit_width;
                let description = res.description.unwrap_or_else(|| "".to_string());
                println!("{typ:<6}│{bits:>5} │ {name:>20} │ {description}");
            }
        }
        ResourceRead(ResourceReadArgs { inst, resource }) => {
            let instance = instance_registry::get_instance_by_name(&mut fvp, inst)?;
            println!("{:>8} │ {}", "value", "name");
            println!("{:═>8}═╪═{:═<35}", "", "");
            for res in resource::get_list(&mut fvp, instance.id, None, None)? {
                if res.name.starts_with(&resource) {
                    let val = resource::read(&mut fvp, instance.id, vec![res.id])?;
                    if !val.data.is_empty() {
                        println!("{:>8x} │ {}", val.data[0], res.name);
                    }
                }
            }
        }
        ChildList(OptionalInstanceArgs { inst }) => {
            let name = match inst.clone() {
                Some(i) => instance_registry::get_instance_by_name(&mut fvp, i)?.name,
                None => String::new(),
            };
            for instance in instance_registry::list_instances(&mut fvp, name.clone())? {
                if instance.name != name {
                    println!("{}", instance.name.trim_start_matches(&name));
                }
            }
        }
        MemoryRead(ReadMemArgs {
            inst,
            addr,
            size,
            group_by,
        }) => {
            let instance = instance_registry::get_instance_by_name(&mut fvp, inst.clone())?;
            let addr = u64::from_str_radix(&addr, 16)?;
            let size = u64::from_str_radix(&size.unwrap_or_else(|| "4".to_string()), 16)?;
            let memory = memory::read(&mut fvp, instance.id, 0, addr, 1, size)?;
            let buf: Vec<_> = memory
                .data
                .into_iter()
                .map(|u| u.to_le_bytes())
                .flatten()
                .collect();
            print_hex_dump(addr, &buf, group_by.unwrap_or(GroupBy::U8));
        }
        Break(ReadMemArgs {
            inst, addr, size, ..
        }) => {
            let sim = instance_registry::get_instance_by_name(
                &mut fvp,
                "framework.SimulationEngine".to_string(),
            )?;
            let instance = instance_registry::get_instance_by_name(&mut fvp, inst.clone())?;
            let addr = u64::from_str_radix(&addr, 16)?;
            let size = size.and_then(|s| u64::from_str_radix(&s, 16).ok());
            let bp = breakpoint::code(&mut fvp, instance.id, addr, size, 0, false, false)?;
            simulation_time::run(&mut fvp, sim.id)?;
            while simulation_time::get(&mut fvp, sim.id)?.running {}
            breakpoint::delete(&mut fvp, instance.id, bp)?;
        }
        Reset => {
            let sim = instance_registry::get_instance_by_name(
                &mut fvp,
                "framework.SimulationEngine".to_string(),
            )?;
            simulation::reset(&mut fvp, sim.id, false)?;
            simulation::wait(&mut fvp, sim.id)?;
        }
        GdbProxy(InstanceArgs { inst }) => {
            let instance = instance_registry::get_instance_by_name(&mut fvp, inst.clone())?;
            let res = resource::get_list(&mut fvp, instance.id, None, None)?;
            if res.iter().any(|r| r.name == "X30") {
                use cornea::gdb::a64::{GdbOverPipe, IrisGdbStub};

                let mut proxy = IrisGdbStub::from_instance(&mut fvp, instance.id)?;
                let mut stub = GdbStub::new(GdbOverPipe::new(stdin(), stdout()));
                eprintln!("Disconnected with {:?}", stub.run(&mut proxy)?);
            } else {
                use cornea::gdb::t32::{GdbOverPipe, IrisGdbStub};

                let mut proxy = IrisGdbStub::from_instance(&mut fvp, instance.id)?;
                let mut stub = GdbStub::new(GdbOverPipe::new(stdin(), stdout()));
                eprintln!("Disconnected with {:?}", stub.run(&mut proxy)?);
            }
        }
    }
    fvp.close()?;
    Ok(())
}
