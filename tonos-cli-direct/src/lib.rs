extern crate tonos_cli;

use tonos_cli::NodeLink;
use tonos_cli::NodeLinkClient;
use ton_api::ton::accountaddress::AccountAddress;
use tokio::runtime::Runtime;

mod client;
mod get_account;

use crate::client::ControlClient;
use crate::get_account::GetAccount;

struct DirectNodeLink;
struct DirectNodeLinkClient {
    client: Option<ControlClient>,
    rt: Runtime
}

impl NodeLink for DirectNodeLink {
    fn create_client(&self, cfg_str: String) -> Box<dyn NodeLinkClient> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build().expect("Can't create tokio runtime");
        let config = serde_json::from_str(&cfg_str).expect("Can't parse config");
        let client = rt.block_on(ControlClient::connect(config)).expect("Can't create client");
        return Box::new(DirectNodeLinkClient { client: Some(client), rt });
    }
    fn name(&self) -> String {
        "direct".to_string()
    }
}

impl NodeLinkClient for DirectNodeLinkClient {
    fn query_accounts(&mut self, addresses: Vec<String>) -> Result<Vec<String>, String> {
        let mut rv = Vec::<String>::new();
        for addr_str in addresses {
            let addr = AccountAddress {
                account_address: addr_str
            };
            let cmd = Box::new(GetAccount{account: addr});
            let (res, _) = self.rt.block_on(self.client.as_mut().unwrap().process_command(&*cmd)).
                map_err(|e| format!("failed ControlClient process_command: {}", e))?;
            rv.push(res);
        }
        Ok(rv)
    }
}

impl Drop for DirectNodeLinkClient {
    fn drop(&mut self) {
        self.client.take().map_or_else(||{}, |v| {
            self.rt.block_on(v.shutdown()).expect("Failed adnl client shutdown");
        });
    }
}

#[no_mangle]
pub fn get_node_link() -> *mut dyn NodeLink {
    // Return a raw pointer to an instance of our plugin
    Box::into_raw(Box::new(DirectNodeLink {}))
}

