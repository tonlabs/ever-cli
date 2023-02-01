use super::echo::Echo;
use super::stdout::Stdout;
use super::{
    AddressInput, AmountInput, ConfirmInput, Menu, NumberInput, SigningBoxInput,
    EncryptionBoxInput, Terminal, UserInfo, InputInterface
};
use crate::config::Config;
use crate::debot::ChainProcessor;
use crate::helpers::TonClient;
use num_bigint::BigInt;
use num_traits::cast::NumCast;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use dengine::prelude::{DebotInterface, DebotInterfaceExecutor};
use ton_client::encoding::{decode_abi_bigint, decode_abi_number};

pub struct SupportedInterfaces {
    client: TonClient,
    interfaces: HashMap<String, Arc<dyn DebotInterface + Send + Sync>>,
}

#[async_trait::async_trait]
impl DebotInterfaceExecutor for SupportedInterfaces {
    fn get_interfaces(&self) -> &HashMap<String, Arc<dyn DebotInterface + Send + Sync>> {
        &self.interfaces
    }
    fn get_client(&self) -> TonClient {
        self.client.clone()
    }
}

/// Helper struct used only inside SupportedInterfaces.
struct InterfaceWrapper {
    processor: Arc<RwLock<ChainProcessor>>,
}
impl InterfaceWrapper {
    fn wrap(
        &self,
        iface: Arc<dyn DebotInterface + Send + Sync>,
    ) -> Arc<dyn DebotInterface + Send + Sync> {
        Arc::new(InputInterface::new(iface, self.processor.clone()))
    }
}

impl SupportedInterfaces {
    pub fn new(client: TonClient, config: &Config, processor: Arc<RwLock<ChainProcessor>>) -> Self {
        let mut interfaces = HashMap::new();

        let iw = InterfaceWrapper { processor: processor.clone() };

        let iface: Arc<dyn DebotInterface + Send + Sync> = iw.wrap(Arc::new(AddressInput::new(config.clone())));
        interfaces.insert(iface.get_id(), iface);

        let iface: Arc<dyn DebotInterface + Send + Sync> = iw.wrap(Arc::new(AmountInput::new()));
        interfaces.insert(iface.get_id(), iface);

        let iface: Arc<dyn DebotInterface + Send + Sync> = iw.wrap(Arc::new(NumberInput::new()));
        interfaces.insert(iface.get_id(), iface);

        let iface: Arc<dyn DebotInterface + Send + Sync> = iw.wrap(Arc::new(ConfirmInput::new()));
        interfaces.insert(iface.get_id(), iface);

        let iface: Arc<dyn DebotInterface + Send + Sync> = Arc::new(Stdout::new());
        interfaces.insert(iface.get_id(), iface);

        let iface: Arc<dyn DebotInterface + Send + Sync> = Arc::new(Echo::new());
        interfaces.insert(iface.get_id(), iface);

        let iface: Arc<dyn DebotInterface + Send + Sync> = iw.wrap(
            Arc::new(Terminal::new(Printer {processor}))
        );
        interfaces.insert(iface.get_id(), iface);

        let iface: Arc<dyn DebotInterface + Send + Sync> = iw.wrap(Arc::new(Menu::new()));
        interfaces.insert(iface.get_id(), iface);

        let iface: Arc<dyn DebotInterface + Send + Sync> = Arc::new(
            SigningBoxInput::new(client.clone(), iw.processor.clone())
        );
        interfaces.insert(iface.get_id(), iface);

        let iface: Arc<dyn DebotInterface + Send + Sync> = iw.wrap(
            Arc::new(UserInfo::new(client.clone(), config.clone()))
        );
        interfaces.insert(iface.get_id(), iface);

        let iface: Arc<dyn DebotInterface + Send + Sync> =
            Arc::new(EncryptionBoxInput::new(client.clone()));
        interfaces.insert(iface.get_id(), iface);

        Self { client, interfaces }
    }
}

pub struct Printer {
    processor: Arc<RwLock<ChainProcessor>>,
}

impl Printer {
    pub async fn print(&self, msg: &str) {
        self.processor.read().await.print(msg);
    }
}

pub fn decode_answer_id(args: &Value) -> Result<u32, String> {
    u32::from_str_radix(
        args["answerId"]
            .as_str()
            .ok_or("answer id not found in argument list".to_string())?,
        10,
    )
    .map_err(|e| format!("{}", e))
}

pub fn decode_arg(args: &Value, name: &str) -> Result<String, String> {
    args[name]
        .as_str()
        .ok_or(format!("\"{}\" not found", name))
        .map(|x| x.to_string())
}

pub fn decode_bool_arg(args: &Value, name: &str) -> Result<bool, String> {
    args[name]
        .as_bool()
        .ok_or(format!("\"{}\" not found", name))
}

pub fn decode_string_arg(args: &Value, name: &str) -> Result<String, String> {
    decode_arg(args, name)
}

pub fn decode_nonce(args: &Value) -> Result<String, String> {
    decode_arg(args, "nonce")
}

pub fn decode_prompt(args: &Value) -> Result<String, String> {
    decode_string_arg(args, "prompt")
}

pub fn decode_num_arg<T>(args: &Value, name: &str) -> Result<T, String>
where
    T: NumCast,
{
    let num_str = decode_arg(args, name)?;
    decode_abi_number::<T>(&num_str)
        .map_err(|e| format!("failed to parse integer \"{}\": {}", num_str, e))
}

pub fn decode_int256(args: &Value, name: &str) -> Result<BigInt, String> {
    let num_str = decode_arg(args, name)?;
    decode_abi_bigint(&num_str)
        .map_err(|e| format!("failed to decode integer \"{}\": {}", num_str, e))
}

pub fn decode_array<F, T>(args: &Value, name: &str, validator: F) -> Result<Vec<T>, String>
where
    F: Fn(&Value) -> Option<T>,
{
    let array = args[name]
        .as_array()
        .ok_or(format!("\"{}\" is invalid: must be array", name))?;
    let mut strings = vec![];
    for elem in array {
        strings.push(validator(elem).ok_or("invalid array element type".to_string())?);
    }
    Ok(strings)
}
