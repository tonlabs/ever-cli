/*
* Copyright 2018-2020 TON DEV SOLUTIONS LTD.
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
use crate::convert::convert_u64_to_tokens;
use crate::helpers::{create_client, load_ton_address, load_abi, TonClient};
use std::io::{self, BufRead, Write};
use std::sync::{Arc, RwLock};
use ton_client::abi::{ Abi, CallSet, ParamsOfEncodeInternalMessage, encode_internal_message};
use ton_client::boc::{ParamsOfParse, parse_message};
use ton_client::crypto::SigningBoxHandle;
use ton_client::debot::{DebotInterfaceExecutor, BrowserCallbacks, DAction, DEngine,
    DebotActivity, DebotInfo, STATE_EXIT, DEBOT_WC};
use ton_client::error::ClientResult;
use std::collections::{HashMap, VecDeque};
use super::{SupportedInterfaces};

/// Stores Debot info needed for DBrowser.
struct DebotEntry {
    abi: Abi,
    dengine: DEngine,
    callbacks: Arc<Callbacks>,
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
    conf: Config,
}

impl TerminalBrowser {
    fn new(client: TonClient, conf: &Config) -> Self {
        Self {
            client: client.clone(),
            msg_queue: Default::default(),
            bots: HashMap::new(),
            interfaces: SupportedInterfaces::new(client.clone(), conf),
            conf: conf.clone(),
        }
    }

    async fn fetch_debot(&mut self, addr: &str, start: bool) -> Result<(), String> {
        let debot_addr = load_ton_address(addr, &self.conf)?;
        let callbacks = Arc::new(Callbacks::new(self.client.clone(), self.conf.clone()));
        let callbacks_ref = Arc::clone(&callbacks);
        let mut dengine = DEngine::new_with_client(
            debot_addr.clone(),
            None,
            self.client.clone(),
            callbacks
        );
        let info: DebotInfo = dengine.init().await?.into();
        let abi_ref = info.dabi.as_ref();
        let abi = load_abi(&abi_ref.ok_or(format!("DeBot ABI is not defined"))?)?;
        Self::print_info(info);
        let mut run_debot = true;
        let _ = terminal_input("Run the DeBot (y/n)?", |val| {
            run_debot = match val.as_str() {
                "y" => true,
                "n" => false,
                _ => Err(format!("invalid enter"))?,
            };
            Ok(())
        });
        if !run_debot {
            return Err(format!("DeBot rejected"));
        }
        if start {
            dengine.start().await?;
        }
        {
            let msgs = &mut callbacks_ref.state.write().unwrap().msg_queue;
            self.msg_queue.append(msgs);
        }

        self.bots.insert(
            debot_addr,
            DebotEntry {
                abi,
                dengine,
                callbacks: callbacks_ref,
            }
        );
        Ok(())
    }

    async fn call_interface(
        &mut self,
        msg: String,
        interface_id: &String,
        debot_addr: &str,
    ) -> Result<(), String> {
        let debot = self.bots.get_mut(debot_addr)
            .ok_or_else(|| "Internal browser error: debot not found".to_owned())?;
        if let Some(result) = self.interfaces.try_execute(&msg, interface_id).await {
            let (func_id, return_args) = result?;
            debug!("response: {} ({})", func_id, return_args);
            let call_set = match func_id {
                0 => None,
                _ => CallSet::some_with_function_and_input(&format!("0x{:x}", func_id), return_args),
            };
            let response_msg = encode_internal_message(
                self.client.clone(),
                ParamsOfEncodeInternalMessage {
                    abi: Some(debot.abi.clone()),
                    src_address: Some(format!("{}:{}", DEBOT_WC, interface_id)),
                    address: Some(debot_addr.to_owned()),
                    call_set,
                    value: "1000000000000000".to_owned(),
                    ..Default::default()
                }
            )
            .await
            .map_err(|e| format!("{}", e))?
            .message;
            let result = debot.dengine.send(response_msg).await;
            let new_msgs = &mut debot.callbacks.state.write().unwrap().msg_queue;
            self.msg_queue.append(new_msgs);
            if let Err(e) = result {
                println!("Debot error: {}", e);
            }
        }

        Ok(())
    }

    async fn call_debot(&mut self, addr: &str, msg: String) -> Result<(), String> {
        if self.bots.get_mut(addr).is_none() {
            self.fetch_debot(addr, false).await?;
        }
        let debot = self.bots.get_mut(addr)
            .ok_or_else(|| "Internal error: debot not found")?;
        debot.dengine.send(msg).await.map_err(|e| format!("Debot failed: {}", e))?;
        let new_msgs = &mut debot.callbacks.state.write().unwrap().msg_queue;
        self.msg_queue.append(new_msgs);
        Ok(())
    }

    fn print_info(info: DebotInfo) {
        println!("DeBot Info:");
        println!("Name   : {}", info.name.unwrap_or_else(|| format!("None")));
        println!("Version: {}", info.version.unwrap_or_else(|| format!("None")));
        println!("Author : {}", info.author.unwrap_or_else(|| format!("None")));
        println!("Publisher: {}", info.publisher.unwrap_or_else(|| format!("None")));
        println!("Support: {}", info.support.unwrap_or_else(|| format!("None")));
        println!("Description: {}", info.caption.unwrap_or_else(|| format!("None")));
        println!("{}", info.hello.unwrap_or_else(|| format!("None")));
    }

}

#[derive(Default)]
struct ActiveState {
    state_id: u8,
    active_actions: Vec<DAction>,
    msg_queue: VecDeque<String>,
}

struct Callbacks {
    config: Config,
    client: TonClient,
    state: Arc<RwLock<ActiveState>>,
}

impl Callbacks {
    pub fn new(client: TonClient, config: Config) -> Self {
        Self { client, config, state: Arc::new(RwLock::new(ActiveState::default())) }
    }

    pub fn select_action(&self) -> Option<DAction> {
        let state = self.state.read().unwrap();
        if state.state_id == STATE_EXIT {
            return None;
        }
        if state.active_actions.len() == 0 {
            debug!("no more actions, exit loop");
            return None;
        }

        loop {
            let res = action_input(state.active_actions.len());
            if res.is_err() {
                println!("{}", res.unwrap_err());
                continue;
            }
            let (n, _, _) = res.unwrap();
            let act = state.active_actions.get(n - 1);
            if act.is_none() {
                println!("Invalid action. Try again.");
                continue;
            }
            return act.map(|a| a.clone());
        }
    }
}

#[async_trait::async_trait]
impl BrowserCallbacks for Callbacks {
    /// Debot asks browser to print message to user
    async fn log(&self, msg: String) {
        println!("{}", msg);
    }

    /// Debot is switched to another context.
    async fn switch(&self, ctx_id: u8) {
        debug!("switched to ctx {}", ctx_id);
        let mut state = self.state.write().unwrap();
        state.state_id = ctx_id;
        if ctx_id == STATE_EXIT {
            return;
        }

        state.active_actions = vec![];
    }

    async fn switch_completed(&self) {
    }

    /// Debot asks browser to show user an action from the context
    async fn show_action(&self, act: DAction) {
        let mut state = self.state.write().unwrap();
        println!("{}) {}", state.active_actions.len() + 1, act.desc);
        state.active_actions.push(act);
    }

    // Debot engine asks user to enter argument for an action.
    async fn input(&self, prefix: &str, value: &mut String) {
        let stdio = io::stdin();
        let mut reader = stdio.lock();
        let mut writer = io::stdout();
        *value = input(prefix, &mut reader, &mut writer);
    }

    /// Debot engine requests keys to sign something
    async fn get_signing_box(&self) -> Result<SigningBoxHandle, String> {
        let mut terminal_box = TerminalSigningBox::new(self.client.clone(), vec![]).await?;
        Ok(terminal_box.leak())
    }

    /// Debot asks to run action of another debot
    async fn invoke_debot(&self, debot: String, action: DAction) -> Result<(), String> {
        debug!("fetching debot {} action {}", &debot, action.name);
        println!("Invoking debot {}", &debot);
        run_debot_browser(&debot, self.config.clone(), false, None).await
    }

    async fn send(&self, message: String) {
        let mut state = self.state.write().unwrap();
        state.msg_queue.push_back(message);
    }

    async fn approve(&self, activity: DebotActivity) -> ClientResult<bool> {
        let mut approved = false;
        println!("--------------------");
        println!("[Permission Request]");
        println!("--------------------");
        match activity {
            DebotActivity::Transaction{msg: _, dst, out, fee, setcode, signkey, signing_box_handle: _} => {
                println!("DeBot is going to create an onchain transaction.\n");
                println!("Details:");
                println!("  account: {}.", dst);
                println!("  Transaction fees: {} tokens.", convert_u64_to_tokens(fee));
                if out.len() > 0 {
                    println!("  Outgoing transfers from account:");
                    for spending in out {
                        println!(
                            "    recipient: {}, amount: {} tokens.",
                            spending.dst,
                            convert_u64_to_tokens(spending.amount),
                        );
                    }
                } else {
                    println!("  No outgoing transfers from account");
                }
                println!("  Message signer public key: {}", signkey);
                if setcode {
                    println!("  Warning: the transaction will change the account smart contract code");
                }
                let _ = terminal_input("Confirm the transaction (y/n)?", |val| {
                    approved = match val.as_str() {
                        "y" => true,
                        "n" => false,
                        _ => Err(format!("invalid enter"))?,
                    };
                    Ok(())
                });
            },
        }
        Ok(approved)
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
            .map(|x| x.parse::<String>().unwrap())
            .collect::<Vec<String>>()
            .len();
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
        .map_err(|_| format!("Oops! Invalid action. Try again, please."))?;
    if n > max {
        Err(format!("Auch! Invalid action. Try again, please."))?;
    }

    Ok((n, argc, argv))
}

/// Launches Terminal DeBot Browser with one DeBot.
///
/// Fetches DeBot by address from blockchain and if `start` is true, starts it in interactive mode.
/// If `init_message` has a value then Browser sends it to DeBot before starting it.
pub async fn run_debot_browser(
    addr: &str,
    config: Config,
    start: bool,
    init_message: Option<String>,
) -> Result<(), String> {
    println!("Connecting to {}", config.url);
    let ton = create_client(&config)?;
    let mut browser = TerminalBrowser::new(ton.clone(), &config);
    browser.fetch_debot(addr, start).await?;
    if let Some(msg) = init_message {
        browser.call_debot(addr, msg).await?;
    }
    loop {
        let mut next_msg = browser.msg_queue.pop_front();
        while let Some(msg) = next_msg {
            let parsed = parse_message(
                ton.clone(),
                ParamsOfParse { boc: msg.clone() },
            )
            .await
            .map_err(|e| format!("{}", e))?
            .parsed;

            let msg_dest = parsed["dst"]
            .as_str()
            .ok_or(format!("parsed message has no dst address"))?;

            let msg_src = parsed["src"]
            .as_str()
            .ok_or(format!("parsed message has no dst address"))?;

            let wc_and_addr: Vec<_> = msg_dest.split(':').collect();
            let id = wc_and_addr[1].to_string();
            let wc = i8::from_str_radix(wc_and_addr[0], 10).map_err(|e| format!("{}", e))?;

            if wc == DEBOT_WC {
                browser.call_interface(msg, &id, msg_src).await?;
            } else {
                browser.call_debot(msg_dest, msg).await?;
            }

            next_msg = browser.msg_queue.pop_front();
        }

        let action = browser.bots.get(addr)
            .ok_or_else(|| "Internal error: debot not found".to_owned())?
            .callbacks
            .select_action();
        match action {
            Some(act) => {
                let debot = browser.bots.get_mut(addr)
                    .ok_or_else(|| "Internal error: debot not found".to_owned())?;
                debot.dengine.execute_action(&act).await?
            },
            None => break,
        }
    }
    println!("Debot Browser shutdown");
    Ok(())
}

#[cfg(test)]
mod tests {}
