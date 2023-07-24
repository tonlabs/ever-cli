use adnl::{
    common::TaggedTlObject, client::{AdnlClient, AdnlClientConfig, AdnlClientConfigJson}
};

use ton_api::{
    serialize_boxed, tag_from_bare_object,
    ton::{
        self, TLObject,
        engine::validator::{ControlQueryError},
        rpc::engine::validator::ControlQuery
    }
};

use ton_types::{error, fail, Result};

/// ControlClient
pub(crate) struct ControlClient{
    adnl: AdnlClient
}

pub(crate) trait SendReceive {
    fn send(&self) -> Result<TLObject>;
    fn receive(&self, answer: TLObject) -> Result<(String, Vec<u8>)> {
        downcast::<ton_api::ton::engine::validator::Success>(answer)?;
        Ok(("success".to_string(), vec![]))
    }
}

//let config = serde_json::from_str(&config).expect("Can't parse config");

pub(crate) fn downcast<T: ton_api::AnyBoxedSerialize>(data: TLObject) -> Result<T> {
    match data.downcast::<T>() {
        Ok(result) => Ok(result),
        Err(obj) => fail!("Wrong downcast {:?} to {}", obj, std::any::type_name::<T>())
    }
}

impl ControlClient {

    /// Connect to server
    pub async fn connect(config: AdnlClientConfigJson) -> Result<Self> {
        let (_, adnl_config) = AdnlClientConfig::from_json_config(config)?;
        Ok(Self {
            adnl: AdnlClient::connect(&adnl_config).await?,
        })
    }

    /// Shutdown client
    pub async fn shutdown(self) -> Result<()> {
        self.adnl.shutdown().await
    }

    /// Process command
    pub async fn process_command(
        &mut self,
        cmd: &dyn SendReceive,
    ) -> Result<(String, Vec<u8>)> {
        let query = cmd.send()?;
        let boxed = ControlQuery {
            data: ton::bytes(serialize_boxed(&query)?)
        };
        #[cfg(feature = "telemetry")]
        let tag = tag_from_bare_object(&boxed);
        let boxed = TaggedTlObject {
            object: TLObject::new(boxed),
            #[cfg(feature = "telemetry")]
            tag
        };
        let answer = self.adnl.query(&boxed).await
            .map_err(|err| error!("Error receiving answer: {}", err))?;
        match answer.downcast::<ControlQueryError>() {
            Err(answer) => match cmd.receive(answer) {
                Err(answer) => fail!("Wrong response to {:?}: {:?}", query, answer),
                Ok(result) => Ok(result)
            }
            Ok(error) => fail!("Error response to {:?}: {:?}", query, error),
        }
    }
}

