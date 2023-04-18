pub mod iris_client {
    use std::collections::{HashMap, HashSet};
    use std::ffi::OsStr;
    use std::io::{BufRead, BufReader, Error as IOError, Write};
    use std::marker::PhantomData;
    use std::net::{SocketAddr, TcpStream};
    use std::process::{Child, Command, Stdio};
    use std::str::FromStr;
    use std::time::Instant;

    use bufstream::BufStream;
    use serde::{de::DeserializeOwned, Deserialize, Serialize};
    use serde_json;

    use crate::instance_registry;

    /// An Iris connection to a fast model.
    pub struct FastModelIris {
        proc: Option<Child>,
        ipc: BufStream<TcpStream>,
        inst_id: Option<u32>,
        pub startup_time: Instant,
        current_msg_id: u32,
        callbacks: HashMap<String, Box<dyn FnMut(serde_json::Value) -> Result<(), IOError>>>,
    }
    pub struct RpcReq<'a, S> {
        pub method: &'a str,
        pub params: &'a S,
    }

    #[derive(Serialize)]
    struct _RpcReq<'a, S: Serialize> {
        jsonrpc: &'a str,
        method: &'a str,
        params: &'a S,
        id: u64,
    }

    #[derive(Deserialize, Debug)]
    #[serde(untagged)]
    pub enum RpcRes {
        Event {
            method: String,
            #[serde(default)]
            params: serde_json::Value,
        },
        Responce {
            // some functions have no return value; others return Null. We treat
            // those the same
            #[serde(default)]
            result: serde_json::Value,
            id: u64,
        },
        Error {
            error: serde_json::Value,
            id: u64,
        },
    }

    #[allow(unused)]
    #[derive(Deserialize, Debug)]
    pub struct AttributeInfo {
        description: Option<String>,
        optional: Option<bool>,
        #[serde(rename = "type")]
        typ: String,
    }

    #[derive(Clone, Copy, Hash, Eq, PartialEq)]
    pub struct MessageHandle<Out>(u64, PhantomData<Out>);

    #[doc(hidden)]
    fn port_from_stdout<B: BufRead>(out: &mut B) -> Result<Option<u16>, IOError> {
        for line in out.lines() {
            let line = line?;
            if let Some(port) = line.strip_prefix("Iris server started listening to port ") {
                return Ok(Some(FromStr::from_str(port).unwrap()));
            }
        }
        Ok(None)
    }

    pub trait IrisOut {
        type Out: DeserializeOwned + std::fmt::Debug;
    }

    #[derive(Deserialize, Debug)]
    pub enum Void {}

    impl IrisOut for () {
        type Out = Void;
    }

    impl FastModelIris {
        /// Construct a Fast Model from command line arguments
        pub fn from_args<I, S>(args: I) -> Result<Self, IOError>
        where
            I: IntoIterator<Item = S>,
            S: AsRef<OsStr>,
        {
            let mut args = args.into_iter();
            let _ = args.next();
            match args.next() {
                Some(comm) => {
                    let mut proc = Command::new(comm)
                        .args(args)
                        .arg("-I")
                        .arg("-p")
                        .stdout(Stdio::piped())
                        .spawn()?;
                    let portnum = {
                        let stdout = proc.stdout.as_mut().unwrap();
                        let mut out = BufReader::new(stdout);
                        port_from_stdout(&mut out)?.unwrap()
                    };
                    Self::from_port(Some(proc), portnum)
                }
                None => {
                    panic!("No fvp command line specified");
                }
            }
        }

        pub fn from_port(proc: Option<Child>, portnum: u16) -> Result<Self, IOError> {
            let startup_time = Instant::now();
            let ipc = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], portnum)))?;
            let ipc = BufStream::new(ipc);
            Ok(Self {
                proc,
                ipc,
                inst_id: None,
                startup_time,
                current_msg_id: 0,
                callbacks: HashMap::new(),
            })
        }

        /// Register this struct as a component within Iris within the attached fast
        /// model. This will negotiate protocl, version and serialization formats.
        pub fn register(&mut self) -> Result<u32, IOError> {
            // Send initial Handshake, including supported serialization.
            self.ipc
                .write(b"CONNECT / IrisRpc/1.0\r\nSupported-Formats: IrisJson\r\n\r\n")?;
            self.ipc.flush()?;
            // Assert that the Iris server supportes the serialization formats that
            // we can send.
            match self.read_formats()? {
                None => {
                    return Err(IOError::new(
                        std::io::ErrorKind::Other,
                        "The Iris server hug up before completing the handshake",
                    ))
                }
                Some(formats) => {
                    if !formats.contains(&"IrisJson".to_string()) {
                        return Err(IOError::new(
                            std::io::ErrorKind::Other,
                            "The Iris server does not support IrisJson",
                        ));
                    }
                }
            }

            // Register ourselves as an object within Iris
            let registration =
                instance_registry::register_instance(self, "cornea".to_string(), true)?;
            self.inst_id = Some(registration.id);
            Ok(registration.id)
        }

        #[doc(hidden)]
        fn read_formats(&mut self) -> Result<Option<Vec<String>>, IOError> {
            for line in BufReader::new(&mut self.ipc).lines() {
                let line = line?;
                if let Some(formats) = line.strip_prefix("Supported-Formats: ") {
                    let formats = formats
                        .split_ascii_whitespace()
                        .map(|x| x.trim_end_matches(",").to_string());
                    return Ok(Some(formats.collect()));
                }
            }
            Ok(None)
        }

        /// Send a message to Iris within the Fast Model. This returns a
        /// MessageHandle that may be passed to the `wait` or `wait_for_many`
        /// methods on this struct.
        pub fn send<'a, M: Serialize + 'a, I: Into<RpcReq<'a, M>>>(
            &mut self,
            message: I,
        ) -> Result<MessageHandle<M>, IOError> {
            let input = vec![message.into()];
            let output = self.send_many(input)?;
            for v in output.into_iter() {
                return Ok(v);
            }
            unreachable!()
        }

        /// Send a batch of messages to Iris within the Fast Model. This returns a
        /// Vec<MessageHandle> that may be passed to the `wait_for_many` method
        /// on this struct.
        pub fn send_many<'a, Itr, Itm, M>(
            &mut self,
            messages: Itr,
        ) -> Result<Vec<MessageHandle<M>>, IOError>
        where
            Itr: IntoIterator<Item = Itm>,
            Itm: Into<RpcReq<'a, M>>,
            M: Serialize + 'a,
        {
            let mut res = Vec::new();
            for msg in messages.into_iter() {
                let RpcReq { method, params } = msg.into();
                let msg = _RpcReq {
                    method,
                    params,
                    id: ((self.inst_id.unwrap_or(0) as u64) << 32) | self.current_msg_id as u64,
                    jsonrpc: "2.0",
                };
                self.current_msg_id += 1;
                let msg_text = serde_json::to_string(&msg).unwrap();
                //eprintln!("-> {:?}", msg_text);
                res.push(MessageHandle(msg.id, PhantomData));
                write!(self.ipc, "IrisJson:{}:{}\n", msg_text.len(), msg_text)?;
            }
            self.ipc.flush()?;
            Ok(res)
        }

        /// Wait for a message with the specified handle. Throws away all other
        /// messages that are read from the channel.
        pub fn wait<M: IrisOut>(
            &mut self,
            msg: MessageHandle<M>,
        ) -> Result<<M as IrisOut>::Out, IOError> {
            let input = vec![msg];
            let output = self.wait_for_many(input)?;
            for v in output.into_iter() {
                return Ok(v);
            }
            Err(IOError::new(
                std::io::ErrorKind::Other,
                "Connection closed before response",
            ))
        }

        /// Wait for all messages within the specified handle set. Throws away all other
        /// messages that are read from the channel.
        pub fn wait_for_many<I, M>(&mut self, msgs: I) -> Result<Vec<<M as IrisOut>::Out>, IOError>
        where
            I: IntoIterator<Item = MessageHandle<M>>,
            M: IrisOut,
        {
            let mut msgs = msgs
                .into_iter()
                .map(|MessageHandle(id, ..)| id)
                .collect::<HashSet<_>>();
            if msgs.len() < 1 {
                return Ok(Vec::new());
            }
            let mut out = Vec::with_capacity(msgs.len());
            for line in (&mut self.ipc).lines() {
                let line = line?;
                if let Some(without_header) = line.strip_prefix("IrisJson:") {
                    let mut parts = without_header.splitn(2, ":");
                    let size = parts.next().map(usize::from_str);
                    let payload = parts.next();
                    match (size, payload) {
                        (Some(size), Some(payload)) => {
                            let size = size.expect("HERE");
                            if payload.len() == size {
                                //eprintln!("<- {:?}",payload);
                                let res: Result<RpcRes, _> = serde_json::from_str(payload);
                                match res {
                                    Ok(RpcRes::Responce { id, result, .. }) => {
                                        if msgs.contains(&id) {
                                            msgs.remove(&id);
                                            out.push(serde_json::from_value(result)?);
                                            if msgs.is_empty() {
                                                return Ok(out);
                                            }
                                        } else {
                                            eprintln!(
                                                "Received unexpected response: {} {:#?}",
                                                id, result
                                            );
                                        }
                                    }
                                    Ok(RpcRes::Event { method, params, .. }) => {
                                        if let Some(cb) = self.callbacks.get_mut(&method) {
                                            cb(params)?;
                                        } else {
                                            eprintln!(
                                                "Warn: Unhandled callback {} {:#?}",
                                                method, params
                                            );
                                        }
                                    }
                                    Ok(RpcRes::Error { error, .. }) => {
                                        return Err(IOError::new(
                                            std::io::ErrorKind::Other,
                                            error.to_string(),
                                        ))
                                    }
                                    Err(_e) => {
                                        return Err(IOError::new(
                                            std::io::ErrorKind::Other,
                                            payload.to_string(),
                                        ))
                                    }
                                }
                            } else {
                                eprintln!("Error: ipc length did not match computed length");
                            }
                        }
                        (Some(_), None) => eprintln!("Error: ipc missing payload"),
                        (None, Some(_)) => {
                            unreachable!("Somehow got something afte a : but nothing before it")
                        }
                        (None, None) => eprintln!("Error: ipc missing length, payload"),
                    }
                } else {
                    eprintln!(
                        "Error: line from ipc in did not start with IrisJson\n{}",
                        line
                    );
                }
            }
            Err(IOError::new(
                std::io::ErrorKind::Other,
                "Connection closed before response",
            ))
        }

        /// Execute an RPC with Iris within the Fast Model.
        pub fn execute<'a, M, I>(&mut self, message: I) -> Result<<M as IrisOut>::Out, IOError>
        where
            M: Serialize + IrisOut + 'a,
            I: Into<RpcReq<'a, M>>,
        {
            self.send(message).and_then(|r| self.wait(r))
        }

        pub fn wait_for_events(&mut self) -> IOError {
            let handle: MessageHandle<()> = MessageHandle(0, PhantomData);
            self.wait(handle).unwrap_err()
        }

        /// Execute a Batch of with Iris within the Fast Model.
        pub fn batch<'a, M, Itr, Itm>(
            &mut self,
            messages: Itr,
        ) -> Result<Vec<<M as IrisOut>::Out>, IOError>
        where
            M: Serialize + IrisOut + 'a,
            Itr: IntoIterator<Item = Itm>,
            Itm: Into<RpcReq<'a, M>>,
        {
            self.send_many(messages).and_then(|r| self.wait_for_many(r))
        }

        #[allow(unused)]
        pub fn close(mut self) -> Result<(), IOError> {
            if let Some(mut proc) = self.proc {
                proc.kill()?;
                proc.wait()?;
            }
            Ok(())
        }

        pub fn register_callback(
            &mut self,
            method: String,
            cb: Box<dyn FnMut(serde_json::Value) -> Result<(), IOError>>,
        ) {
            self.callbacks.insert(method, cb);
        }
    }
}

macro_rules! iris_rpc_fn {
    ($name:ident $method:literal $reqname:ident {$($(#[$reqattr: meta])? $reqident: ident: $reqty: ty),*} -> $resname:ty) => {
        pub fn $name(fvp: &mut crate::iris_client::FastModelIris, $($reqident: $reqty),*) -> Result<$resname, std::io::Error> {
            let resource_handle = fvp.send(crate::iris_client::RpcReq {
                method: $method,
                params: &$reqname{
                    $($reqident),*
                },
            })?;
            fvp.wait(resource_handle)
        }

        #[derive(serde::Serialize)]
        pub struct $reqname {
            $($(#[$reqattr])? pub $reqident: $reqty),*
        }

        impl<'a> From<&'a $reqname> for crate::iris_client::RpcReq<'a, $reqname> {
            fn from(params: &'a $reqname) -> Self {
                Self {
                    method: $method,
                    params
                }
            }
        }

        impl crate::iris_client::IrisOut for $reqname {
            type Out = $resname;
        }
    };

    ($name:ident $method:literal $reqname:ident {$($(#[$reqattr: meta])? $reqident: ident: $reqty: ty,)*} -> $resname:ty) => {
        iris_rpc_fn!($name $method
            $reqname {
                $($(#[$reqattr])? $reqident: $reqty),*
            } -> $resname
        );
    };
}

pub mod instance_registry {
    use crate::iris_client::AttributeInfo;
    use serde::Deserialize;
    use std::collections::HashMap;

    iris_rpc_fn!(register_instance "instanceRegistry_registerInstance"
        RegisterInstance {
            #[serde(rename = "instName")]
            inst_name: String,
            uniquify: bool,
        } -> RegisterInstanceRes
    );

    #[derive(Deserialize, Debug, Clone)]
    pub struct Instance {
        #[serde(rename = "instId")]
        pub id: u32,
        #[serde(rename = "instName")]
        pub name: String,
    }

    #[derive(Deserialize, Debug)]
    pub struct RegisterInstanceRes {
        #[serde(rename = "instName")]
        pub name: String,
        #[serde(rename = "instId")]
        pub id: u32,
    }

    #[allow(unused)]
    #[derive(Deserialize, Debug)]
    pub struct FunctionInfo {
        args: HashMap<String, AttributeInfo>,
        description: String,
        retval: AttributeInfo,
    }

    iris_rpc_fn!(list_instances "instanceRegistry_getList"
        ListInsnances { prefix: String } -> Vec<Instance>
    );

    iris_rpc_fn!(get_instance_by_id "instanceRegistry_getInstanceInfoByInstId"
        GetInstByIdReq {
            #[serde(rename = "aInstId")]
            id: u32,
        } -> Instance
    );
    iris_rpc_fn!(get_instance_by_name "instanceRegistry_getInstanceInfoByName"
        GetInstByNameReq {
            #[serde(rename = "instName")]
            name: String,
        } -> Instance
    );
    iris_rpc_fn!(get_function_info "instance_getFunctionInfo"
        GetFuncInfoReq {
            #[serde(rename = "instId")]
            id: u32,
            prefix: String,
        } -> HashMap<String, FunctionInfo>
    );
}

pub mod memory {
    use crate::iris_client::AttributeInfo;
    use serde::Deserialize;
    use serde_json::Value;
    use std::collections::HashMap;

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "camelCase")]
    pub struct Space {
        pub attrib: Option<HashMap<String, AttributeInfo>>,
        pub attrib_defaults: Option<HashMap<String, AttributeInfo>>,
        pub cannonical_msn: Option<u64>,
        pub description: Option<String>,
        pub endianness: Option<String>,
        pub max_addr: Option<u64>,
        pub min_addr: Option<u64>,
        pub name: String,
        #[serde(rename = "spaceId")]
        pub id: u64,
    }

    iris_rpc_fn!(spaces "memory_getMemorySpaces"
        GetFuncInfoReq {
            #[serde(rename = "instId")]
            id: u32
        } -> Vec<Space>
    );

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "camelCase")]
    pub struct ReadRes {
        pub data: Vec<u64>,
        pub error: Option<Value>,
    }

    iris_rpc_fn!(
        read "memory_read"
            MemoryReadReq {
                #[serde(rename = "instId")]
                id: u32,
                #[serde(rename = "spaceId")]
                space: u64,
                address: u64,
                #[serde(rename = "byteWidth")]
                width: u64,
                count: u64,
            } -> ReadRes
    );
}

pub mod breakpoint {
    use crate::iris_client::FastModelIris;
    use serde::{Deserialize, Serialize};
    use std::io::Error as IOError;

    #[allow(unused)]
    #[derive(Deserialize, Debug)]
    pub struct ConditionInfo {
        name: String,
        #[serde(rename = "type")]
        typ: String,
        description: String,
        #[serde(rename = "bptTypes")]
        bpt_types: Option<Vec<Type>>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename_all = "camelCase")]
    pub enum Type {
        Code,
        Data,
        Register,
    }

    iris_rpc_fn!(additional_conditions "breakpoint_getAdditionalConditions"
        GetFuncInfoReq {
            #[serde(rename = "instId")]
            id: u32,
            #[serde(rename = "type")]
            typ: Option<Type>
        } -> Vec<ConditionInfo>
    );

    iris_rpc_fn!(set "breakpoint_set"
        Set {
            #[serde(rename = "instId")]
            id: u32,
            address: u64,
            #[serde(rename = "rwMode", skip_serializing_if = "Option::is_none")]
            rw_mode: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            size: Option<u64>,
            #[serde(rename = "spaceId",skip_serializing_if = "Option::is_none")]
            space_id: Option<u64>,
            #[serde(rename = "syncEc")]
            sync: bool,
            #[serde(rename = "type")]
            typ: Type,
            #[serde(rename = "dontStop")]
            dont_stop: bool,
        } -> u64
    );

    iris_rpc_fn!(delete "breakpoint_delete"
        Delete {
            #[serde(rename = "instId")]
            instance: u32,
            #[serde(rename = "bptId")]
            breakpoint: u64,
        } -> ()
    );

    pub fn code(
        fvp: &mut FastModelIris,
        id: u32,
        addr: u64,
        size: Option<u64>,
        space_id: u64,
        sync: bool,
        dont_stop: bool,
    ) -> Result<u64, IOError> {
        set(
            fvp,
            id,
            addr,
            None,
            size,
            Some(space_id),
            sync,
            Type::Code,
            dont_stop,
        )
    }
}

pub mod checkpoint {
    iris_rpc_fn!(save "checkpoint_save"
        Save {
            #[serde(rename = "instId")]
            id: u32,
            #[serde(rename = "checkpointDir")]
            dir: String
        } -> ()
    );
    iris_rpc_fn!(restore "checkpoint_restore"
        Restore {
            #[serde(rename = "instId")]
            id: u32,
            #[serde(rename = "checkpointDir")]
            dir: String
        } -> ()
    );
}

pub mod step {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(rename_all = "camelCase")]
    pub enum Unit {
        Instruction,
        Cycle,
    }
    iris_rpc_fn!(setup "step_setup"
        Setup {
            #[serde(rename = "instId")]
            id: u32,
            steps: u64,
            unit: Unit
        } -> ()
    );
    iris_rpc_fn!(remaining "step_getRemainingSteps"
        Remain {
            #[serde(rename = "instId")]
            id: u32,
            unit: Unit
        } -> u64
    );
}

pub mod simulation_time {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "camelCase")]
    pub struct Time {
        pub ticks: u64,
        pub tick_hz: u64,
        pub running: bool,
    }
    iris_rpc_fn!(run "simulationTime_run"
        Run {
            #[serde(rename = "instId")]
            id: u32
        } -> ()
    );
    iris_rpc_fn!(stop "simulationTime_stop"
        Stop {
            #[serde(rename = "instId")]
            id: u32
        } -> ()
    );
    iris_rpc_fn!(get "simulationTime_get"
        Get {
            #[serde(rename = "instId")]
            id: u32
        } -> Time
    );
}

pub mod simulation {
    iris_rpc_fn!(reset "simulation_reset"
        Reset {
            #[serde(rename = "instId")]
            id: u32,
            #[serde(rename = "allowPartialReset")]
            allow_partial: bool,
        } -> ()
    );
    iris_rpc_fn!(wait "simulation_waitForInstantiation"
         Wait {
            #[serde(rename = "instId")]
            id: u32,
        } -> ()
    );
}

pub mod event_stream {
    iris_rpc_fn!(create "eventStream_create"
        Create {
            #[serde(rename = "instId", skip_serializing_if = "Option::is_none")]
            id: Option<u32>,
            disable: bool,
            #[serde(rename = "ecInstId")]
            to_id: u32,
            #[serde(rename = "evSrcId")]
            source: u32,
            #[serde(rename = "ringBuffer")]
            buffer: bool,
        } -> u64
    );

    iris_rpc_fn!(trace_ranges "eventStream_setTraceRanges"
        TraceRanges {
            #[serde(rename = "instId")]
            id: u32,
            #[serde(rename = "esId")]
            es_id: u64,
            aspect: String,
            ranges: Vec<u64>,
        } -> ()
    );
}

pub mod event {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    pub struct Field {
        pub name: String,
        #[serde(rename = "type")]
        pub typ: String,
        pub size: u64,
        pub description: Option<String>,
    }

    #[derive(Deserialize, Debug)]
    pub struct SourceInfo {
        pub description: Option<String>,
        pub name: String,
        #[serde(rename = "evSrcId")]
        pub id: u32,
        pub fields: Vec<Field>,
    }

    iris_rpc_fn!(source "event_getEventSource"
        Source { #[serde(rename = "instId")] id: u32, name: String} -> SourceInfo
    );

    iris_rpc_fn!(sources "event_getEventSources"
        Sources { #[serde(rename = "instId")] id: u32, } -> Vec<SourceInfo>
    );
}

pub mod resource {
    use serde::Deserialize;
    use serde_json::Value;
    #[derive(Deserialize, Debug)]
    pub struct ResourceInfo {
        #[serde(rename = "bitWidth")]
        pub bit_width: u64,
        pub cname: String,
        pub description: Option<String>,
        pub name: String,
        pub parent_id: Option<u64>,
        #[serde(rename = "rscId")]
        pub id: u64,
        #[serde(rename = "parameterInfo")]
        pub parameter_info: Option<Value>,
        #[serde(rename = "registerInfo")]
        pub register_info: Option<Value>,
        #[serde(rename = "rwMode")]
        pub rw_mode: Option<String>,
    }

    iris_rpc_fn!(get_list "resource_getList"
        GetList {
            #[serde(rename = "instId")]
            id: u32,
            #[serde(skip_serializing_if = "Option::is_none")]
            group: Option<String>,
            #[serde(rename = "rscId", skip_serializing_if = "Option::is_none")]
            resource_id: Option<u32>,
        } -> Vec<ResourceInfo>
    );

    #[derive(Deserialize, Debug)]
    pub struct ResourceRead {
        pub data: Vec<u64>,
    }

    iris_rpc_fn!(read "resource_read"
        Read {
            #[serde(rename = "instId")]
            id: u32,
            #[serde(rename = "rscIds")]
            resource_ids: Vec<u64>,
        } -> ResourceRead
    );
}

pub use iris_client::FastModelIris;
pub mod gdb;
