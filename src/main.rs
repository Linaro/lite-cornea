use std::convert::TryInto;
use std::io::{stdin, stdout};
use std::str::FromStr;
use std::iter;

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
    /// List all available event streams
    EventSources(InstanceArgs),
    /// Get delailed information about an event source
    EventFields(ResourceReadArgs),
    /// Log events as they occur
    EventLog(ResourceOptionArgs),
    /// Describe the matching registers of an instance
    RegisterList(InstanceArgs),
    /// Tabulate memory spaces
    MemorySpaces(InstanceArgs),
    /// Tabulate memory sideband info
    MemoryInfo(SidebandArgs),
    /// Translate an address into another memory space
    MemoryTranslate(TranslateArgs),
    /// Print the children of this instance
    ChildList(OptionalInstanceArgs),
    /// Read memory from the prespective of an instance
    MemoryRead(ReadMemArgs),
    /// Break at a pc range
    Break(ReadMemArgs),
    /// Reset the platform
    Reset,
    /// Read matching registers from an instance
    RegisterRead(ResourceReadArgs),
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
struct SidebandArgs {
    /// The name of the instance to read from
    inst: String,
    /// Address to print from
    addr: String,
}

#[derive(Parser, Debug)]
struct TranslateArgs {
    /// The name of the instance to read from
    inst: String,
    /// Address to print from
    addr: String,
    /// Memory space that the address belongs to
    from: SpaceArg,
    /// Memory space that the result belongs to
    to: SpaceArg,
}

#[derive(Parser, Debug)]
struct SpaceArg {
    inner: String,
}

impl FromStr for SpaceArg {
    type Err = String;
    fn from_str(frm: &str) -> Result<Self, String> {
        Ok(Self {
            inner: frm.to_string(),
        })
    }
}

impl SpaceArg {
    fn into_id(self, fvp: &mut FastModelIris, inst: u32) -> Result<u64, std::io::Error> {
        let num = u64::from_str(&self.inner);
        if let Ok(n) = num {
            return Ok(n);
        }
        let spaces = memory::spaces(fvp, inst)?;
        match spaces
            .iter()
            .find(|i| i.name.to_lowercase() == self.inner.to_lowercase())
        {
            Some(spc) => Ok(spc.id),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Space {} not found", self.inner),
            )),
        }
    }
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
#[derive(Parser, Debug)]
struct ResourceOptionArgs {
    /// The name of the instance to read from
    inst: String,
    /// Resource to print from
    resource: Option<String>,
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
                stop: false,
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

fn mismatch(xs: &[u8], ys: &[u8]) -> usize {
    mismatch_chunks::<128>(xs, ys)
}

fn mismatch_chunks<const N: usize>(xs: &[u8], ys: &[u8]) -> usize {
    let off = iter::zip(xs.chunks_exact(N), ys.chunks_exact(N))
        .take_while(|(x, y)| x == y)
        .count()
        * N;
    off + iter::zip(&xs[off..], &ys[off..])
        .take_while(|(x, y)| x == y)
        .count()
}

fn common_prefix_len<'a, I: IntoIterator<Item=&'a str>>(haystack: I) -> usize {
    let mut haystack = haystack.into_iter();
    let start = match haystack.next() {
        Some(start) => start,
        None => return 0,
    };
    let prefix = |e: &str| {
        mismatch(e.as_bytes(), start.as_bytes())
    };
    haystack.map(prefix).min().unwrap_or(0)
}

fn find_instance(fvp: &mut FastModelIris, name: String) -> Result<instance_registry::Instance, std::io::Error> {
    if let Ok(inst) = instance_registry::get_instance_by_name(fvp, name.clone()) {
        return Ok(inst);
    }
    let name = &name.trim_start_matches(".");
    let instance_list = instance_registry::list_instances(fvp, "component".to_string())?;
    let prefix = common_prefix_len(instance_list.iter().map(|i| i.name.as_str()));
    for inst in instance_list {
        let n = &inst.name[prefix..].trim_start_matches(".");
        if n == name {
            return Ok(inst);
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::Other, "Instance not found"))
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

fn get_iris(port: Option<u16>) -> Result<FastModelIris, std::io::Error> {
    if let Some(port) = port {
        FastModelIris::from_port(None, port)
    } else {
        let mut fvp = FastModelIris::from_port(None, 7100);
        for port in 7101..7105 {
            if fvp.is_ok() {
                break;
            }
            fvp = FastModelIris::from_port(None, port)
        }
        fvp
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    let mut fvp = get_iris(args.port)?;
    let my_id = fvp.register()?;
    use Command::*;
    match args.command {
        RegisterList(InstanceArgs { inst }) => {
            let instance = find_instance(&mut fvp, inst)?;
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
        EventSources(InstanceArgs { inst }) => {
            let instance = find_instance(&mut fvp, inst)?;
            let sources = event::sources(&mut fvp, instance.id)?;
            let name_len = sources.iter().map(|s| s.name.len()).max().unwrap_or(0);
            println!("{:>name_len$} │ {}", "name", "description");
            println!("{:═>name_len$}═╪═{:═<20}", "", "");
            for res in sources {
                let name = res.name;
                let description = res.description.unwrap_or_else(|| "".to_string());
                println!("{name:>name_len$} │ {description}");
            }
        }
        EventFields(ResourceReadArgs { inst, resource }) => {
            let instance = find_instance(&mut fvp, inst)?;
            let source = event::source(&mut fvp, instance.id, resource)?;
            println!(
                "{:<6}│{:^6}│ {:>20} │ {}",
                "type", "size", "name", "description"
            );
            println!("{:═<6}╪{:═^6}╪═{:═>20}═╪═{:═<20}", "", "", "", "");
            for res in source.fields {
                let typ = res.typ;
                let name = res.name;
                let bits = res.size;
                let description = res.description.unwrap_or_else(|| "".to_string());
                println!("{typ:<6}│{bits:>5} │ {name:>20} │ {description}");
            }
        }
        EventLog(ResourceOptionArgs {
            inst,
            resource: Some(resource),
         }) => {
            let instance = find_instance(&mut fvp, inst)?;
            let source = event::source(&mut fvp, instance.id, resource.clone())?;
            let _stream =
                event_stream::create(&mut fvp, Some(instance.id), false, my_id, source.id, false, false)?;
            fvp.register_callback(
                format!("ec_{}", resource),
                Box::new(|params| Ok(println!("{}", params))),
            );
            fvp.wait_for_events();
        }
        EventLog(ResourceOptionArgs {
            inst,
            resource: None,
        }) => {
            let instance = find_instance(&mut fvp, inst)?;
            let sources = event::sources(&mut fvp, instance.id)?;
            for s in sources {
                let _stream =
                    event_stream::create(&mut fvp, Some(instance.id), false, my_id, s.id, false, false);
            }
            fvp.wait_for_events();
        }
        RegisterRead(ResourceReadArgs { inst, resource }) => {
            let instance = find_instance(&mut fvp, inst)?;
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
                Some(i) => find_instance(&mut fvp, i)?.name,
                None => String::new(),
            };
            for instance in instance_registry::list_instances(&mut fvp, name.clone())? {
                if instance.name != name {
                    println!("{}", instance.name.trim_start_matches(&name));
                }
            }
        }
        MemoryInfo(SidebandArgs { inst, addr }) => {
            let instance = find_instance(&mut fvp, inst)?;
            let addr = u64::from_str_radix(&addr, 16)?;
            let info = memory::sideband_info(&mut fvp, instance.id, 0, addr)?;
            println!(
                "{:>8} │ {:>8} │ {:>8} │ {:>8} │ {:>2}",
                "Start", "End addr", "Phys", "IPA", "NX"
            );
            println!(
                "{:>8x} │ {:>8x} │ {:>8x} │ {:>8x} │ {:>2}",
                info.region_start,
                info.region_end,
                info.physical_address,
                info.ipa,
                if info.no_execute { "Y" } else { "" }
            );
        }
        MemoryTranslate(TranslateArgs {
            inst,
            addr,
            from,
            to,
        }) => {
            let instance = find_instance(&mut fvp, inst)?;
            let addr = u64::from_str_radix(&addr, 16)?;
            let from = from.into_id(&mut fvp, instance.id)?;
            let to = to.into_id(&mut fvp, instance.id)?;
            let out_addr = memory::translate(&mut fvp, instance.id, addr, from, to)?.address;
            for oa in out_addr {
                println!("{oa:>8x}");
            }
        }
        MemorySpaces(InstanceArgs { inst }) => {
            let instance = find_instance(&mut fvp, inst)?;
            let spaces = memory::spaces(&mut fvp, instance.id)?;
            let name_len = spaces.iter().map(|s| s.name.len()).max().unwrap_or(0);
            println!("{:>name_len$} │ {}", "name", "description");
            println!("{:═>name_len$}═╪═{:═<35}", "", "");
            for space in &spaces {
                println!(
                    "{:>name_len$} │ {}",
                    space.name,
                    space.description.as_deref().unwrap_or("")
                );
            }
        }
        MemoryRead(ReadMemArgs {
            inst,
            addr,
            size,
            group_by,
        }) => {
            let instance = find_instance(&mut fvp, inst)?;
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
            let bp = breakpoint::code(&mut fvp, instance.id, addr, size, 0, false)?;
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
            let instance = find_instance(&mut fvp, inst)?;
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
