/*
* Copyright 2018-2021 TON DEV SOLUTIONS LTD.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific TON DEV software governing permissions and
* limitations under the License.
*/
use super::term_signing_box::TerminalSigningBox;
use crate::config::Config;
use crate::helpers::{create_client, load_ton_address, load_abi, TonClient};
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use serde_json::json;
use ton_client::abi::{ Abi, CallSet, ParamsOfEncodeInternalMessage, ParamsOfDecodeMessage,
    encode_internal_message, decode_message};
use ton_client::boc::{ParamsOfParse, parse_message};
use ton_client::debot::{DebotInterfaceExecutor, DEngine, DebotInfo, DEBOT_WC};
use std::collections::{HashMap, VecDeque};
use super::{Callbacks, ChainLink, PipeChain, ChainProcessor, SupportedInterfaces};

const BROWSER_ID: &str = "0000000000000000000000000000000000000000000000000000000000000000";
/// Stores Debot info needed for DBrowser.
struct DebotEntry {
    abi: Abi,
    dengine: DEngine,
    callbacks: Arc<Callbacks>,
    info: DebotInfo,
}

/// Top level object. Created only once.
struct TerminalBrowser {
    client: TonClient,
    /// common message queue for both inteface calls and invoke calls (from different debots).
    msg_queue: VecDeque<String>,
    /// Map of instantiated Debots. [addr] -> entry.
    /// New debots are created by invoke requests.
    bots: HashMap<String, DebotEntry>,
    /// Set of intrefaces implemented by current DBrowser.
    interfaces: SupportedInterfaces,
    config: Config,
    processor: Arc<tokio::sync::RwLock<ChainProcessor>>,
    /// Indicates if Browser will interact with the user or not.
    interactive: bool,
    /// Browser exit argument. Initialized only if DeBot sends message to the DeBot Browser address.
    pub exit_arg: Option<serde_json::Value>,
}

impl TerminalBrowser {
    async fn new(client: TonClient, addr: &str, config: Config, pipechain: PipeChain) -> Result<Self, String> {
        let processor = ChainProcessor::new(pipechain);
        let start = processor.default_start();
        let interactive = processor.interactive();
        let call_set = processor.initial_call_set();
        let mut init_message = processor.initial_msg();

        let processor = Arc::new(tokio::sync::RwLock::new(processor));
        let mut browser = Self {
            client: client.clone(),
            msg_queue: Default::default(),
            bots: HashMap::new(),
            interfaces: SupportedInterfaces::new(client.clone(), &config, processor.clone()),
            config,
            processor,
            interactive,
            exit_arg: None,
        };
        let _ = browser.fetch_debot(addr, start, !interactive).await?;
        let abi = browser.bots.get(addr)
            .ok_or(format!("DeBot not found: address {}", addr))?
            .abi.clone();

        if !start && init_message.is_none() {
            init_message = Some(
                encode_internal_message(
                    browser.client.clone(),
                    ParamsOfEncodeInternalMessage {
                        abi: Some(abi),
                        address: Some(addr.to_owned()),
                        src_address: Some(format!("{}:{}", DEBOT_WC, BROWSER_ID)),
                        call_set,
                        value: "1000000000000000".to_owned(),
                        ..Default::default()
                    },
                )
                .map_err(|e| format!("{}", e))?
                .message,
            );
        }

        if let Some(msg) = init_message {
            browser.call_debot(addr, msg).await?;
        }

        Ok(browser)
    }

    async fn fetch_debot(&mut self, addr: &str, call_start: bool, autorun: bool) -> Result<String, String> {
        let debot_addr = load_ton_address(addr, &self.config)?;
        let callbacks = Arc::new(
            Callbacks::new(self.client.clone(), self.config.clone(), self.processor.clone())
        );
        let callbacks_ref = Arc::clone(&callbacks);
        let mut dengine = DEngine::new_with_client(
            debot_addr.clone(),
            None,
            self.client.clone(),
            callbacks
        );
        let info: DebotInfo = dengine.init().await?.into();
        let abi_version = info.dabi_version.clone();
        let abi_ref = info.dabi.as_ref();
        let def_config = Config::default();
        let abi = load_abi(abi_ref.ok_or("DeBot ABI is not defined".to_string())?, &def_config).await?;
        if !autorun {
            Self::print_info(&info);
        }
        let mut run_debot = autorun;
        if !run_debot {
            let _ = terminal_input("Run the DeBot (y/n)?", |val| {
                run_debot = match val.as_str() {
                    "y" => true,
                    "n" => false,
                    _ => return Err("invalid enter".to_string()),
                };
                Ok(())
            });
        }
        if !run_debot {
            return Err("DeBot rejected".to_string());
        }
        if call_start {
            dengine.start().await?;
        }
        callbacks_ref.take_messages(&mut self.msg_queue);

        self.bots.insert(
            debot_addr,
            DebotEntry {
                abi,
                dengine,
                callbacks: callbacks_ref,
                info,
            }
        );
        Ok(abi_version)
    }

    async fn call_interface(
        &mut self,
        msg: String,
        interface_id: String,
        debot_addr: &str,
    ) -> Result<(), String> {
        let debot = self.bots.get_mut(debot_addr)
            .ok_or_else(|| "Internal browser error: debot not found".to_owned())?;
        if let Some(result) = self.interfaces.try_execute(&msg, &interface_id, &debot.info.dabi_version).await {
            let (func_id, return_args) = result?;
            log::debug!("response: {} ({})", func_id, return_args);
            let call_set = match func_id {
                0 => None,
                _ => CallSet::some_with_function_and_input(&format!("0x{:x}", func_id), return_args),
            };
            let response_msg = encode_internal_message(
                self.client.clone(),
                ParamsOfEncodeInternalMessage {
                    abi: Some(debot.abi.clone()),
                    address: Some(debot_addr.to_owned()),
                    call_set,
                    value: "1000000000000000".to_owned(),
                    ..Default::default()
                }
            )
            .map_err(|e| format!("{}", e))?
            .message;
            let result = debot.dengine.send(response_msg).await;
            debot.callbacks.take_messages(&mut self.msg_queue);
            if let Err(e) = result {
                println!("Debot error: {}", e);
            }
        }

        Ok(())
    }

    async fn call_debot(&mut self, addr: &str, msg: String) -> Result<(), String> {
        if self.bots.get_mut(addr).is_none() {
            self.fetch_debot(addr, false, !self.interactive).await?;
        }
        let debot = self.bots.get_mut(addr).ok_or("Internal error: debot not found")?;
        debot.dengine.send(msg).await.map_err(|e| format!("Debot failed: {}", e))?;
        debot.callbacks.take_messages(&mut self.msg_queue);
        Ok(())
    }

    fn print_info(info: &DebotInfo) {
        println!("DeBot Info:");
        fn print(field: &Option<String>) -> &str {
            field.as_ref().map(|v| v.as_str()).unwrap_or("None")
        }
        println!("Name   : {}", print(&info.name));
        println!("Version: {}", print(&info.version));
        println!("Author : {}", print(&info.author));
        println!("Publisher: {}", print(&info.publisher));
        println!("Support: {}", print(&info.support));
        println!("Description: {}", print(&info.caption));
        println!("{}", print(&info.hello));
    }

    async fn set_exit_arg(&mut self, message: String, _debot_addr: &str) -> Result<(), String> {
        let abi = self.processor.read().await.abi();
        let arg = if let Some(abi) = abi {
            let decoded = decode_message(
                self.client.clone(),
                ParamsOfDecodeMessage { 
                    abi,
                    message,
                    ..Default::default()
                 },
            ).map_err(|e| format!("{}", e))?;
            decoded.value.unwrap_or(json!({}))
        } else {
            json!({"message": message})
        };
        self.exit_arg = Some(arg);
        Ok(())
    }

}

pub(crate) fn input<R, W>(prefix: &str, reader: &mut R, writer: &mut W) -> String
where
    R: BufRead,
    W: Write,
{
    let mut input_str = "".to_owned();
    let mut argc = 0;
    while argc == 0 {
        println!("{}", prefix);
        if let Err(e) = writer.flush() {
            println!("failed to flush: {}", e);
            return input_str;
        }
        if let Err(e) = reader.read_line(&mut input_str) {
            println!("failed to read line: {}", e);
            return input_str;
        }
        argc = input_str
            .split_whitespace()
            .count();
    }
    input_str.trim().to_owned()
}

pub(crate) fn terminal_input<F>(prompt: &str, mut validator: F) -> String
where
    F: FnMut(&String) -> Result<(), String>
{
    let stdio = io::stdin();
    let mut reader = stdio.lock();
    let mut writer = io::stdout();
    let mut value = input(prompt, &mut reader, &mut writer);
    while let Err(e) = validator(&value) {
        println!("{}. Try again.", e);
        value = input(prompt, &mut reader, &mut writer);
    }
    value
}
pub fn action_input(max: usize) -> Result<(usize, usize, Vec<String>), String> {
    let mut a_str = String::new();
    let mut argc = 0;
    let mut argv = vec![];
    println!();
    while argc == 0 {
        print!("debash$ ");
        let _ = io::stdout().flush();
        io::stdin()
            .read_line(&mut a_str)
            .map_err(|e| format!("failed to read line: {}", e))?;
        argv = a_str
            .split_whitespace()
            .map(|x| x.parse::<String>().expect("parse error"))
            .collect::<Vec<String>>();
        argc = argv.len();
    }
    let n = usize::from_str_radix(&argv[0], 10)
        .map_err(|_| "Oops! Invalid action. Try again, please.".to_string())?;
    if n > max {
        return Err("Auch! Invalid action. Try again, please.".to_string());
    }

    Ok((n, argc, argv))
}

/// Starts Terminal DeBot Browser with main DeBot.
///
/// Fetches DeBot by address from blockchain and runs it according to pipechain.
pub async fn run_debot_browser(
    addr: &str,
    config: Config,
    mut pipechain: PipeChain,
    signkey_path: Option<String>,
) -> Result<Option<serde_json::Value>, String> {
    if !config.is_json {
        println!("Network: {}", config.url);
    }
    let ton = create_client(&config)?;

    if let Some(path) = signkey_path {
        let input = std::io::BufReader::new(path.as_bytes());
        let mut sbox = TerminalSigningBox::new(ton.clone(), vec![], Some(input)).await?;
        let sbox_handle = sbox.leak();
        for cl in pipechain.chain.iter_mut() {
            if let ChainLink::SigningBox{handle} = cl {
                *handle = sbox_handle.0;
            }
        }
    }
    let mut browser = TerminalBrowser::new(ton.clone(), addr, config, pipechain).await?;
    loop {
        let mut next_msg = browser.msg_queue.pop_front();
        while let Some(msg) = next_msg {
            let parsed = parse_message(
                ton.clone(),
                ParamsOfParse { boc: msg.clone() },
            )
            .map_err(|e| format!("{}", e))?
            .parsed;

            let msg_dest = parsed["dst"].as_str()
                .ok_or("invalid message in the queue: no dst address".to_string())?;

            let msg_src = parsed["src"].as_str()
                .ok_or("invalid message in the queue: no src address".to_string())?;

            let wc_and_addr: Vec<_> = msg_dest.split(':').collect();
            let id = wc_and_addr[1].to_string();
            let wc = i8::from_str_radix(wc_and_addr[0], 10).map_err(|e| format!("{}", e))?;

            if wc == DEBOT_WC {
                if id == BROWSER_ID {
                    // Message from DeBot to Browser
                    browser.set_exit_arg(msg, msg_src).await?;
                } else {
                    browser.call_interface(msg, id, msg_src).await?;
                }
            } else {
                browser.call_debot(msg_dest, msg).await?;
            }

            next_msg = browser.msg_queue.pop_front();
        }

        // Next block is deprecated. Remove it
        let not_found_err = "Internal error: DeBot not found";
        let action = browser.bots.get(addr)
            .ok_or_else(|| not_found_err.to_owned())?
            .callbacks
            .select_action();
        match action {
            Some(act) => {
                let debot = browser.bots.get_mut(addr)
                    .ok_or_else(|| not_found_err.to_owned())?;
                debot.dengine.execute_action(&act).await?
            },
            None => break,
        }
        // ---------------------------------------
    }

    Ok(browser.exit_arg)
}

#[cfg(test)]
mod tests {}
