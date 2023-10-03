/*
 * Copyright 2018-2023 EverX.
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
extern crate reqwest;
use crate::call;
use crate::config::Config;
use crate::convert;
use crate::crypto::load_keypair;
use crate::deploy::prepare_deploy_message_params;
use crate::helpers::{
    create_client_local, create_client_verbose, load_file_with_url, load_ton_address, now_ms,
};
use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use serde_json::json;
use ton_client::abi::{
    encode_message_body, Abi, AbiContract, AbiParam, CallSet, ParamsOfEncodeMessageBody,
};

const SAFEMULTISIG_LINK: &str = "https://github.com/tonlabs/ton-labs-contracts/blob/master/solidity/safemultisig/SafeMultisigWallet.tvc?raw=true";
const SETCODEMULTISIG_LINK: &str = "https://github.com/tonlabs/ton-labs-contracts/blob/master/solidity/setcodemultisig/SetcodeMultisigWallet.tvc?raw=true";
const SAFEMULTISIG_V2_LINK: &str =
    "https://github.com/EverSurf/contracts/blob/main/multisig2/build/SafeMultisig.tvc?raw=true";
const SETCODEMULTISIG_V2_LINK: &str =
    "https://github.com/EverSurf/contracts/blob/main/multisig2/build/SetcodeMultisig.tvc?raw=true";

pub const MSIG_ABI: &str = r#"{
	"ABI version": 2,
	"header": ["pubkey", "time", "expire"],
	"functions": [
		{
			"name": "constructor",
			"inputs": [
				{"name":"owners","type":"uint256[]"},
				{"name":"reqConfirms","type":"uint8"}
			],
			"outputs": [
			]
		},
		{
			"name": "acceptTransfer",
			"inputs": [
				{"name":"payload","type":"bytes"}
			],
			"outputs": [
			]
		},
		{
			"name": "sendTransaction",
			"inputs": [
				{"name":"dest","type":"address"},
				{"name":"value","type":"uint128"},
				{"name":"bounce","type":"bool"},
				{"name":"flags","type":"uint8"},
				{"name":"payload","type":"cell"}
			],
			"outputs": [
			]
		},
		{
			"name": "submitTransaction",
			"inputs": [
				{"name":"dest","type":"address"},
				{"name":"value","type":"uint128"},
				{"name":"bounce","type":"bool"},
				{"name":"allBalance","type":"bool"},
				{"name":"payload","type":"cell"}
			],
			"outputs": [
				{"name":"transId","type":"uint64"}
			]
		},
		{
			"name": "confirmTransaction",
			"inputs": [
				{"name":"transactionId","type":"uint64"}
			],
			"outputs": [
			]
		},
		{
			"name": "isConfirmed",
			"inputs": [
				{"name":"mask","type":"uint32"},
				{"name":"index","type":"uint8"}
			],
			"outputs": [
				{"name":"confirmed","type":"bool"}
			]
		},
		{
			"name": "getParameters",
			"inputs": [
			],
			"outputs": [
				{"name":"maxQueuedTransactions","type":"uint8"},
				{"name":"maxCustodianCount","type":"uint8"},
				{"name":"expirationTime","type":"uint64"},
				{"name":"minValue","type":"uint128"},
				{"name":"requiredTxnConfirms","type":"uint8"}
			]
		},
		{
			"name": "getTransaction",
			"inputs": [
				{"name":"transactionId","type":"uint64"}
			],
			"outputs": [
				{"components":[{"name":"id","type":"uint64"},{"name":"confirmationsMask","type":"uint32"},{"name":"signsRequired","type":"uint8"},{"name":"signsReceived","type":"uint8"},{"name":"creator","type":"uint256"},{"name":"index","type":"uint8"},{"name":"dest","type":"address"},{"name":"value","type":"uint128"},{"name":"sendFlags","type":"uint16"},{"name":"payload","type":"cell"},{"name":"bounce","type":"bool"}],"name":"trans","type":"tuple"}
			]
		},
		{
			"name": "getTransactions",
			"inputs": [
			],
			"outputs": [
				{"components":[{"name":"id","type":"uint64"},{"name":"confirmationsMask","type":"uint32"},{"name":"signsRequired","type":"uint8"},{"name":"signsReceived","type":"uint8"},{"name":"creator","type":"uint256"},{"name":"index","type":"uint8"},{"name":"dest","type":"address"},{"name":"value","type":"uint128"},{"name":"sendFlags","type":"uint16"},{"name":"payload","type":"cell"},{"name":"bounce","type":"bool"}],"name":"transactions","type":"tuple[]"}
			]
		},
		{
			"name": "getTransactionIds",
			"inputs": [
			],
			"outputs": [
				{"name":"ids","type":"uint64[]"}
			]
		},
		{
			"name": "getCustodians",
			"inputs": [
			],
			"outputs": [
				{"components":[{"name":"index","type":"uint8"},{"name":"pubkey","type":"uint256"}],"name":"custodians","type":"tuple[]"}
			]
		}
	],
	"data": [
	],
	"events": [
		{
			"name": "TransferAccepted",
			"inputs": [
				{"name":"payload","type":"bytes"}
			],
			"outputs": [
			]
		}
	]
}"#;

pub const TRANSFER_WITH_COMMENT: &str = r#"{
	"ABI version": 1,
	"functions": [
		{
			"name": "transfer",
			"id": "0x00000000",
			"inputs": [{"name":"comment","type":"bytes"}],
			"outputs": []
		}
	],
	"events": [],
	"data": []
}"#;

const LOCAL_GIVER_TRANSFER: &str = r#"{
	"ABI version": 1,
	"functions": [
		{
			"name": "sendGrams",
			"inputs": [
				{"name": "dest", "type": "address"},
				{"name": "amount", "type": "uint64"}
			],
			"outputs": []
		}
	],
	"events": [],
	"data": []
}"#;

const LOCAL_GIVER_ADDR: &str = "0:841288ed3b55d9cdafa806807f02a0ae0c169aa5edfe88a789a6482429756a94";

#[derive(Default)]
pub struct CallArgs {
    params: serde_json::Value,
    func_name: String,
    image: Option<Vec<u8>>,
}

impl CallArgs {
    pub async fn submit(matches: &ArgMatches<'_>) -> Result<Self, String> {
        let dest = matches
            .value_of("DEST")
            .map(|s| s.to_owned())
            .ok_or("--dst parameter is not defined".to_string())?;
        let value = matches
            .value_of("VALUE")
            .ok_or("--value parameter is not defined".to_string())?;
        let value = convert::convert_token(value)?;
        let comment = matches.value_of("PURPOSE").map(|s| s.to_owned());
        let body = if let Some(ref txt) = comment {
            encode_transfer_body(txt).await?
        } else {
            "".to_owned()
        };
        Self::submit_with_args(matches, &dest, &value, true, body).await
    }

    pub async fn submit_with_args(
        matches: &ArgMatches<'_>,
        dest: &str,
        value: &str,
        bounce: bool,
        payload: String,
    ) -> Result<Self, String> {
        let v2 = matches.is_present("V2");
        if v2 {
            // TODO parse stateinit arg
        }
        let params = json!({
            "dest": dest,
            "value": value,
            "bounce": bounce,
            "allBalance": false,
            "payload": payload,
        });

        Ok(Self {
            params,
            func_name: "submitTransaction".to_owned(),
            ..Default::default()
        })
    }

    pub async fn deploy(matches: &ArgMatches<'_>) -> Result<Self, String> {
        let is_setcode = matches.is_present("SETCODE");
        let v2 = matches.is_present("V2");

        let target = if v2 {
            if is_setcode {
                SETCODEMULTISIG_V2_LINK
            } else {
                SAFEMULTISIG_V2_LINK
            }
        } else if is_setcode {
            SETCODEMULTISIG_LINK
        } else {
            SAFEMULTISIG_LINK
        };

        let image = load_file_with_url(target, 30000).await?;

        let owners = matches.value_of("OWNERS").map(|owners| {
            owners
                .replace(['[', ']', '\"', '\''], "")
                .replace("0x", "")
                .split(',')
                .map(|o| format!("0x{}", o))
                .collect::<Vec<String>>()
        });

        let mut params = json!({
            "owners": owners,
            "reqConfirms": matches.value_of("CONFIRMS").unwrap_or("1"),
        });

        if v2 {
            let lifetime = matches
                .value_of("LIFETIME")
                .map(|s| s.parse::<u64>())
                .unwrap_or(Ok(0))
                .map_err(|e| e.to_string())?;
            params["lifetime"] = json!(lifetime);
        }

        Ok(Self {
            params,
            func_name: "constructor".to_owned(),
            image: Some(image),
        })
    }
}

pub struct MultisigArgs {
    addr: String,
    abi: Abi,
    call_args: CallArgs,
    keys: String,
}

impl MultisigArgs {
    pub fn new(
        matches: &ArgMatches<'_>,
        config: &Config,
        call_args: CallArgs,
    ) -> Result<Self, String> {
        let address = matches
            .value_of("MSIG")
            .map(|s| s.to_owned())
            .or_else(|| config.wallet.clone())
            .ok_or("multisig address is not defined".to_string())?;
        let keys = matches
            .value_of("SIGN")
            .or_else(|| matches.value_of("KEYS"))
            .map(|s| s.to_owned())
            .or_else(|| config.keys_path.clone())
            .ok_or("sign key is not defined".to_string())?;
        let v2 = matches.is_present("V2");

        let addr = load_ton_address(&address, config)?;
        let mut abi = serde_json::from_str::<AbiContract>(MSIG_ABI).unwrap_or_default();
        if v2 {
            abi.version = Some("2.3".to_owned());
            if let Some(f) = abi
                .functions
                .iter_mut()
                .find(|e| &e.name == "submitTransaction")
            {
                f.inputs.push(AbiParam {
                    name: "stateInit".to_owned(),
                    param_type: "optional(cell)".to_owned(),
                    components: vec![],
                });
            }
            if let Some(f) = abi.functions.iter_mut().find(|e| &e.name == "constructor") {
                f.inputs.push(AbiParam {
                    name: "lifetime".to_owned(),
                    param_type: "uint32".to_owned(),
                    components: vec![],
                });
            }
        }

        Ok(Self {
            addr,
            call_args,
            abi: Abi::Contract(abi),
            keys,
        })
    }
    pub fn address(&self) -> &str {
        &self.addr
    }
    pub fn params(&self) -> &serde_json::Value {
        &self.call_args.params
    }
    pub fn abi(&self) -> &Abi {
        &self.abi
    }
    pub fn abi_string(&self) -> String {
        if let Abi::Contract(ref abi) = self.abi {
            serde_json::to_string(abi).unwrap()
        } else {
            unreachable!();
        }
    }
    pub fn func_name(&self) -> &str {
        &self.call_args.func_name
    }
    pub fn keys(&self) -> &str {
        &self.keys
    }
    pub fn image(&self) -> Option<&[u8]> {
        self.call_args.image.as_deref()
    }
    pub async fn execute(self, config: &Config) -> Result<serde_json::Value, String> {
        call::call_contract_with_result(
            config,
            self.address(),
            &self.abi_string(),
            self.func_name(),
            &self.params().to_string(),
            Some(self.keys.clone()),
            false,
        )
        .await
    }
}

pub fn create_multisig_command<'a, 'b>() -> App<'a, 'b> {
    let v2_arg = Arg::with_name("V2")
        .long("--v2")
        .help("Force to interact with wallet account as multisig v2.");
    let bounce_arg = Arg::with_name("BOUNCE")
        .long("--bounce")
        .short("-b")
        .help("Send bounce message to destination account.");

    let keys_arg = Arg::with_name("KEYS")
        .long("--keys")
        .short("-k")
        .takes_value(true)
        .help("Path to the file with a keypair.");

    SubCommand::with_name("multisig")
        .about("Multisignature wallet commands.")
        .setting(AppSettings::AllowNegativeNumbers)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .subcommand(SubCommand::with_name("send")
            .setting(AppSettings::AllowLeadingHyphen)
            .about("Transfer funds from the wallet to the recipient.")
            .arg(Arg::with_name("MSIG")
                .long("--addr")
                .takes_value(true)
                .help("Wallet address. If undefined then config.wallet is used."))
            .arg(Arg::with_name("DEST")
                .long("--dest")
                .takes_value(true)
                .help("Recipient address."))
            .arg(Arg::with_name("VALUE")
                .long("--value")
                .takes_value(true)
                .help("Amount of funds to transfer (in evers)."))
            .arg(Arg::with_name("PURPOSE")
                .long("--purpose")
                .takes_value(true)
                .help("Optional, comment attached to transfer."))
            .arg(Arg::with_name("SIGN")
                .long("--sign")
                .takes_value(true)
                .help("Seed phrase or path to file with keypair."))
            .arg(bounce_arg)
            .arg(v2_arg.clone()))
        .subcommand(SubCommand::with_name("deploy")
            .setting(AppSettings::AllowLeadingHyphen)
            .about("Deploys a wallet with a given public key. By default, deploys a SafeMultisig with one custodian, which can be tuned with flags.")
            .arg(keys_arg)
            .arg(Arg::with_name("SETCODE")
                .long("--setcode")
                .help("Deploy SetcodeMultisig instead of SafeMultisig."))
            .arg(Arg::with_name("VALUE")
                .long("--local")
                .takes_value(true)
                .short("-l")
                .help("Perform a preliminary call of local giver to initialize contract with given value."))
            .arg(Arg::with_name("OWNERS")
                .long("--owners")
                .takes_value(true)
                .short("-o")
                .help("Array of wallet owners public keys. Note: deployer could be not included in this case. If not specified the only owner is contract deployer."))
            .arg(Arg::with_name("CONFIRMS")
                .long("--confirms")
                .takes_value(true)
                .short("-c")
                .help("Number of confirmations required for executing transaction. Default value is 1."))
            .arg(v2_arg))
}

pub async fn multisig_command(m: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    if let Some(m) = m.subcommand_matches("send") {
        return multisig_send_command(m, config).await;
    }
    if let Some(m) = m.subcommand_matches("deploy") {
        return multisig_deploy_command(m, config).await;
    }
    Err("unknown multisig command".to_owned())
}

async fn multisig_send_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let call_args = CallArgs::submit(matches).await?;
    let common_args = MultisigArgs::new(matches, config, call_args)?;
    send(config, common_args).await
}

pub async fn encode_transfer_body(text: &str) -> Result<String, String> {
    encode_message_body(
        create_client_local()?,
        ParamsOfEncodeMessageBody {
            abi: Abi::Json(TRANSFER_WITH_COMMENT.to_owned()),
            call_set: CallSet::some_with_function_and_input(
                "transfer",
                json!({
                    "comment": hex::encode(text.as_bytes())
                }),
            )
            .ok_or("failed to create CallSet with specified parameters")?,
            is_internal: true,
            ..Default::default()
        },
    )
    .await
    .map_err(|e| format!("failed to encode transfer body: {}", e))
    .map(|r| r.body)
}

async fn send(config: &Config, args: MultisigArgs) -> Result<(), String> {
    let result = args.execute(config).await?;
    if !config.is_json {
        println!("Succeeded.");
    }
    call::print_json_result(result, config)
}

async fn multisig_deploy_command(matches: &ArgMatches<'_>, config: &Config) -> Result<(), String> {
    let call_args = CallArgs::deploy(matches).await?;
    let args = MultisigArgs::new(matches, config, call_args)?;

    let keys = load_keypair(args.keys())?;
    let mut params = args.params().clone();
    if params["owners"].is_null() {
        params["owners"] = json!(vec![format!("0x{}", &keys.public)]);
    }
    let (msg, address) = prepare_deploy_message_params(
        args.image().unwrap_or_default(),
        args.abi().clone(),
        "constructor".to_string(),
        now_ms(),
        &params.to_string(),
        Some(keys),
        config.wc,
    )
    .await?;

    if !config.is_json {
        println!("Wallet address: {}", address);
    }

    let ton = create_client_verbose(config)?;

    if let Some(value) = matches.value_of("VALUE") {
        let params = format!(r#"{{"dest":"{}","amount":"{}"}}"#, address, value);
        call::call_contract_with_client(
            ton.clone(),
            config,
            LOCAL_GIVER_ADDR,
            LOCAL_GIVER_TRANSFER,
            "sendGrams",
            &params,
            None,
            false,
        )
        .await?;
    }

    let res = call::process_message(ton.clone(), msg, config)
        .await
        .map_err(|e| format!("{:#}", e));

    if res.is_err() {
        if res
            .clone()
            .err()
            .unwrap()
            .contains("Account does not exist.")
        {
            if !config.is_json {
                println!("Your account should have initial balance for deployment. Please transfer some value to your wallet address before deploy.");
            } else {
                println!("{{");
                println!("  \"Error\": \"Your account should have initial balance for deployment. Please transfer some value to your wallet address before deploy.\",");
                println!("  \"Address\": \"{}\"", address);
                println!("}}");
            }
            return Ok(());
        }
        return Err(res.err().unwrap());
    }

    if !config.is_json {
        println!("Wallet successfully deployed");
        println!("Wallet address: {}", address);
    } else {
        println!("{{");
        println!("  \"Address\": \"{}\"", address);
        println!("}}");
    }

    Ok(())
}
