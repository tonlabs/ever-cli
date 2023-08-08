use serde_json::Value;
use super::{ApproveKind, PipeChain, ChainLink};
use std::vec::IntoIter;
use ton_client::debot::DebotActivity;
use ton_client::abi::{CallSet, Abi};

#[derive(Debug)]
pub enum ProcessorError {
    InterfaceCallNeeded,
    NoMoreChainlinks,
    UnexpectedChainLinkKind,
    UnexpectedInterface,
    UnexpectedMethod,
    InteractiveApproveNeeded,
    // TODO:
    // UnexpectedApproveKind,
}

pub struct ChainProcessor {
    pipechain: PipeChain,
    chain_iter: IntoIter<ChainLink>,
}

impl ChainProcessor {
    pub fn new(mut pipechain: PipeChain ) -> Self {
        let chain_vec = std::mem::take(&mut pipechain.chain);
        Self { pipechain, chain_iter: chain_vec.into_iter() }
    }

    pub fn abi(&self) -> Option<Abi> {
        self.pipechain.abi.clone().map(|v| Abi::Json(v.to_string()))
    }

    pub fn interactive(&self) -> bool {
        !self.pipechain.quiet
    }

    pub fn default_start(&self) -> bool {
        self.pipechain.init_method == "start"
    }

    pub fn print(&self, message: &str) {
        if self.interactive() {
            println!("{}", message);
        }
    }

    pub fn initial_msg(&self) -> Option<String> {
        self.pipechain.init_msg.clone()
    }

    pub fn initial_call_set(&self) -> Option<CallSet> {
        if self.pipechain.init_msg.is_some() {
            return None;
        }
        if self.default_start() {
            return None;
        }
        match &self.pipechain.init_args {
            Some(args) => CallSet::some_with_function_and_input(&self.pipechain.init_method, args.clone()),
            None => CallSet::some_with_function(&self.pipechain.init_method),
        }

    }

    pub fn next_input(
        &mut self,
        in_interface: &str,
        in_method: &str,
        _in_params: &Value
    ) -> Result<Option<Value>, ProcessorError> {
        let chlink = self.chain_iter.next().ok_or(
            if self.interactive() {
                ProcessorError::InterfaceCallNeeded
            } else {
                ProcessorError::NoMoreChainlinks
            }
        )?;

        match chlink {
            ChainLink::Input {interface, method, params, mandatory} => {
                if interface != in_interface {
                    if !mandatory {
                        self.next_input(in_interface, in_method, _in_params)
                    } else {
                        Err(ProcessorError::UnexpectedInterface)
                    }
                } else if method != in_method {
                    Err(ProcessorError::UnexpectedMethod)
                } else {
                    Ok(params)
                }
            },
            _ => Err(ProcessorError::UnexpectedChainLinkKind),
        }
    }

    pub fn next_signing_box(&mut self) -> Result<u32, ProcessorError> {
        let chlink = self.chain_iter.next().ok_or(
            if self.interactive() {
                ProcessorError::InterfaceCallNeeded
            } else {
                ProcessorError::NoMoreChainlinks
            }
        )?;

        match chlink {
            ChainLink::SigningBox {handle} => {
                Ok(handle)
            },
            _ => Err(ProcessorError::UnexpectedChainLinkKind),
        }
    }

    pub fn next_approve(&mut self, activity: &DebotActivity) -> Result<bool, ProcessorError> {

        let app_kind = match activity {
            DebotActivity::Transaction {..} => ApproveKind::ApproveOnChainCall,
        };
        let auto_approve = self.pipechain.auto_approve.as_ref().map(|vec| vec.iter().any(|x| *x == app_kind));

        let chlink = self.chain_iter.next();
        if chlink.is_none() {
            if let Some(auto_approve) = auto_approve {
                return Ok(auto_approve);
            } else if self.interactive() {
                return Err(ProcessorError::InteractiveApproveNeeded);
            } else {
                return Ok(false);
            }
        }

        // TODO: ?
        let chlink = chlink.unwrap();
        match chlink {
            ChainLink::OnchainCall { approve, iflq: _, ifeq: _ } => {
                match activity {
                    DebotActivity::Transaction {msg: _, dst: _, out: _, fee: _, setcode: _, signkey: _, signing_box_handle: _} => {
                        Ok(approve)
                    }
                }
            },
            _ => Err(ProcessorError::UnexpectedChainLinkKind)
        }
    }
}

