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
use crate::config::Config;
use crate::{convert, print_args};
use crate::depool_abi::{DEPOOL_ABI, PARTICIPANT_ABI};
use crate::helpers::{
    create_client,
    create_client_local,
    create_client_verbose,
    load_abi,
    load_ton_address,
    now,
    TonClient,
    answer_filter,
    events_filter,
    print_message,
};
use crate::multisig::{MultisigArgs, CallArgs};
use clap::{App, ArgMatches, SubCommand, Arg, AppSettings};

use serde_json::json;
use ton_client::abi::{ParamsOfEncodeMessageBody, CallSet, ParamsOfDecodeMessageBody};
use ton_client::net::{OrderBy, ParamsOfQueryCollection, ParamsOfWaitForCollection, SortDirection};
use crate::call;
use std::collections::HashMap;

pub fn create_depool_command<'a, 'b>() -> App<'a, 'b> {
    let wallet_arg = Arg::with_name("MSIG")
        .takes_value(true)
        .long("--wallet")
        .short("-w")
        .help("Multisig wallet address.");
    let value_arg = Arg::with_name("VALUE")
        .takes_value(true)
        .long("--value")
        .short("-v")
        .help("Value in tons.");
    let keys_arg = Arg::with_name("SIGN")
        .takes_value(true)
        .long("--sign")
        .short("-s")
        .help("Seed phrase or path to file with keypair which must be used to sign message to multisig wallet.");
    let total_period_arg = Arg::with_name("TPERIOD")
        .takes_value(true)
        .long("--total")
        .short("-t")
        .help("Total period of vesting stake (days).");
    let withdrawal_period_arg = Arg::with_name("WPERIOD")
        .takes_value(true)
        .long("--withdrawal")
        .short("-i")
        .help("Payment period of vesting stake (days).");
    let beneficiary_arg = Arg::with_name("BENEFICIARY")
        .takes_value(true)
        .long("--beneficiary")
        .short("-b")
        .help("Smart contract address which will own lock stake rewards.");
    let donor_arg = Arg::with_name("DONOR")
        .takes_value(true)
        .long("--donor")
        .short("-d")
        .help("Donor smart contract address.");
    let dest_arg = Arg::with_name("DEST")
        .takes_value(true)
        .long("--dest")
        .short("-d")
        .help("Address of the destination smart contract.");
    let wait_answer = Arg::with_name("WAIT_ANSWER")
        .long("--wait-answer")
        .short("-a")
        .help("Wait for depool answer when calling a depool function.");
    let v2_arg = Arg::with_name("V2")
        .long("--v2")
        .help("Force to interpret wallet account as multisig v2.");

    SubCommand::with_name("depool")
        .about("DePool commands.")
        .setting(AppSettings::AllowLeadingHyphen)
        .setting(AppSettings::DontCollapseArgsInUsage)
        .arg(Arg::with_name("ADDRESS")
            .takes_value(true)
            .long("--addr")
            .help("DePool contract address. If omitted, then config.addr is used."))
        .arg(wait_answer.clone())
        .subcommand(SubCommand::with_name("donor")
            .about(r#"Top level command for specifying donor for exotic stakes in depool."#)
            .subcommand(SubCommand::with_name("vesting")
                .about("Set the address from which participant can receive a vesting stake.")
                .setting(AppSettings::AllowLeadingHyphen)
                .arg(wallet_arg.clone())
                .arg(keys_arg.clone())
                .arg(donor_arg.clone())
                .arg(wait_answer.clone())
                .arg(v2_arg.clone()))
            .subcommand(SubCommand::with_name("lock")
                .about("Set the address from which participant can receive a lock stake.")
                .setting(AppSettings::AllowLeadingHyphen)
                .arg(wallet_arg.clone())
                .arg(keys_arg.clone())
                .arg(donor_arg.clone())
                .arg(wait_answer.clone())
                .arg(v2_arg.clone())))
        .subcommand(SubCommand::with_name("answers")
            .about("Prints depool answers")
            .setting(AppSettings::AllowLeadingHyphen)
            .arg(wallet_arg.clone())
            .arg(Arg::with_name("SINCE")
                .takes_value(true)
                .long("--since")
                .short("-s")
                .help("Prints answers since this unixtime.")) )
        .subcommand(SubCommand::with_name("stake")
            .about(r#"Top level command for managing stakes in depool. Uses a supplied multisignature wallet to send internal message with stake to depool."#)
            .subcommand(SubCommand::with_name("ordinary")
                .about("Deposits an ordinary stake in the depool from the multisignature wallet.")
                .setting(AppSettings::AllowLeadingHyphen)
                .arg(wallet_arg.clone())
                .arg(value_arg.clone())
                .arg(keys_arg.clone())
                .arg(wait_answer.clone())
                .arg(v2_arg.clone()))
            .subcommand(SubCommand::with_name("vesting")
                .about("Deposits a vesting stake in the depool from the multisignature wallet.")
                .setting(AppSettings::AllowLeadingHyphen)
                .arg(wallet_arg.clone())
                .arg(value_arg.clone())
                .arg(keys_arg.clone())
                .arg(wait_answer.clone())
                .arg(total_period_arg.clone())
                .arg(withdrawal_period_arg.clone())
                .arg(beneficiary_arg.clone())
                .arg(v2_arg.clone()))
            .subcommand(SubCommand::with_name("lock")
                .about("Deposits a lock stake in the depool from the multisignature wallet.")
                .setting(AppSettings::AllowLeadingHyphen)
                .arg(wallet_arg.clone())
                .arg(value_arg.clone())
                .arg(keys_arg.clone())
                .arg(wait_answer.clone())
                .arg(total_period_arg.clone())
                .arg(withdrawal_period_arg.clone())
                .arg(beneficiary_arg.clone())
                .arg(v2_arg.clone()))
            .subcommand(SubCommand::with_name("transfer")
                .about("Transfers ownership of the wallet stake to another contract.")
                .setting(AppSettings::AllowLeadingHyphen)
                .arg(wallet_arg.clone())
                .arg(value_arg.clone())
                .arg(keys_arg.clone())
                .arg(wait_answer.clone())
                .arg(dest_arg.clone())
                .arg(v2_arg.clone()))
            .subcommand(SubCommand::with_name("remove")
                .about("Withdraws an ordinary stake from the current pooling round of the depool to the multisignature wallet.")
                .setting(AppSettings::AllowLeadingHyphen)
                .arg(wallet_arg.clone())
                .arg(value_arg.clone())
                .arg(keys_arg.clone())
                .arg(wait_answer.clone())
                .arg(v2_arg.clone()))
            .subcommand(SubCommand::with_name("withdrawPart")
                .about("Withdraws part of the stake after round completion.")
                .setting(AppSettings::AllowLeadingHyphen)
                .arg(wallet_arg.clone())
                .arg(value_arg.clone())
                .arg(wait_answer.clone())
                .arg(keys_arg.clone())
                .arg(v2_arg.clone())))
        .subcommand(SubCommand::with_name("replenish")
            .about("Transfers funds from the multisignature wallet to the depool contract (NOT A STAKE).")
            .setting(AppSettings::AllowLeadingHyphen)
            .arg(wallet_arg.clone())
            .arg(value_arg.clone())
            .arg(keys_arg.clone())
            .arg(v2_arg.clone()))
        .subcommand(SubCommand::with_name("ticktock")
            .about("Calls depool 'ticktock()' function to update its state. 1 ton is attached to this call (change will be returned).")
            .setting(AppSettings::AllowLeadingHyphen)
            .arg(wallet_arg.clone())
            .arg(keys_arg.clone())
            .arg(v2_arg.clone()))
        .subcommand(SubCommand::with_name("withdraw")
            .about("Allows to disable auto investment of the stake into the next round and withdraw all the stakes after round completion.")
            .setting(AppSettings::AllowLeadingHyphen)
            .subcommand(SubCommand::with_name("on")
                .setting(AppSettings::AllowLeadingHyphen)
                .arg(wallet_arg.clone())
                .arg(wait_answer.clone())
                .arg(keys_arg.clone())
                .arg(v2_arg.clone()))
            .subcommand(SubCommand::with_name("off")
                .setting(AppSettings::AllowLeadingHyphen)
                .arg(wallet_arg.clone())
                .arg(wait_answer.clone())
                .arg(keys_arg.clone())
                .arg(v2_arg.clone())))
        .subcommand(SubCommand::with_name("events")
            .about("Prints depool events.")
            .setting(AppSettings::AllowLeadingHyphen)
            .arg(Arg::with_name("SINCE")
                .takes_value(true)
                .long("--since")
                .short("-s")
                .help("Prints events since this unixtime."))
            .arg(Arg::with_name("WAITONE")
                .long("--wait-one")
                .short("-w")
                .help("Waits until new event will be emitted.")) )
}

///
/// Depool command
/// 
/// Stores parameters for request from wallet to depool
struct DepoolCmd<'a> {
    /// Reference to command line arguments
    m: &'a ArgMatches<'a>,
    /// Reference to depool address
    depool: &'a str,
    /// Amount of nanoevers to send from wallet to depool
    value: u64,
    /// Payload for message from wallet to depool.
    /// Encodes one of depool API functions.
    body: String,
    /// Reference to global console config
    config: &'a Config,
    /// Request with answer from depool
    with_answer: bool,
}

impl<'a> DepoolCmd<'a> {
    pub async fn stake_ordinary(
        m: &'a ArgMatches<'_>,
        config: &'a Config,
        depool: &'a str
    ) -> Result<DepoolCmd<'a>, String> {
        let mut value = parse_value(m)?;
        let body = encode_add_ordinary_stake(value).await?;
        value += Self::depool_fee(config)?;
        Ok(Self {m, depool, value, body, config, with_answer: true})
    }

    pub async fn stake_vesting(
        m: &'a ArgMatches<'_>,
        config: &'a Config,
        depool: &'a str,
        with_lock: bool
    ) -> Result<DepoolCmd<'a>, String> {
        let mut value = parse_value(m)?;
        let withdrawal_period = m.value_of("WPERIOD")
            .ok_or("withdrawal period is not defined.".to_string())?;
        let total_period = m.value_of("TPERIOD")
            .ok_or("total period is not defined.".to_string())?;
        let beneficiary = m.value_of("BENEFICIARY")
            .ok_or("beneficiary is not defined.".to_string())?;
        let beneficiary = load_ton_address(beneficiary, config)?;

        let period_checker = |v| {
            if v > 0 && v <= 36500 {
                Ok(v)
            } else {
                Err("period cannot be more than 36500 days".to_string())
            }
        };
        let wperiod = u32::from_str_radix(withdrawal_period, 10)
            .map_err(|e| format!("invalid withdrawal period: {}", e))
            .and_then(period_checker)?;
        let tperiod = u32::from_str_radix(total_period, 10)
            .map_err(|e| format!("invalid total period: {}", e))
            .and_then(period_checker)?;
        let wp = wperiod * 86400;
        let tp = tperiod * 86400;

        let body = if with_lock {
            encode_add_lock_stake(value, &beneficiary, tp, wp).await?
        } else {
            encode_add_vesting_stake(value, &beneficiary, tp, wp).await?
        };
        value += Self::depool_fee(config)?;
        Ok(Self {m, depool, value, body, config, with_answer: true})
    }

    pub async fn stake_remove(
        m: &'a ArgMatches<'_>,
        config: &'a Config,
        depool: &'a str
    ) -> Result<DepoolCmd<'a>, String> {
        let stake = parse_value(m)?;
        let body = encode_remove_stake(stake).await?;
        let value = Self::depool_fee(config)?;
        Ok(Self {m, depool, value, body, config, with_answer: true})
    }

    pub async fn stake_withdraw_part(
        m: &'a ArgMatches<'_>,
        config: &'a Config,
        depool: &'a str
    ) -> Result<DepoolCmd<'a>, String> {
        let stake = parse_value(m)?;
        let body = encode_withdraw_stake(stake).await?;
        let value = Self::depool_fee(config)?;
        Ok(Self {m, depool, value, body, config, with_answer: true})
    }

    pub async fn stake_transfer(
        m: &'a ArgMatches<'_>,
        config: &'a Config,
        depool: &'a str
    ) -> Result<DepoolCmd<'a>, String> {
        let dest = m.value_of("DEST")
            .ok_or("destination address is not defined.".to_owned())?;
        let dest = load_ton_address(dest, config)?;
        let stake = parse_value(m)?;
        let body = encode_transfer_stake(&dest, stake).await?;
        let value = Self::depool_fee(config)?;
        Ok(Self {m, depool, value, body, config, with_answer: true})
    }

    pub async fn donor(
        m: &'a ArgMatches<'_>,
        config: &'a Config,
        depool: &'a str,
        for_vesting: bool,
    ) -> Result<DepoolCmd<'a>, String> {
        let donor = m.value_of("DONOR")
            .ok_or("donor is not defined.".to_string())?;
        let body = encode_set_donor(for_vesting, donor).await?;
        let value = Self::depool_fee(config)?;
        Ok(Self {m, depool, value, body, config, with_answer: true})
    }

    pub async fn withdraw(
        m: &'a ArgMatches<'_>,
        config: &'a Config,
        depool: &'a str,
        enable: bool,
    ) -> Result<DepoolCmd<'a>, String> {
        let body = encode_set_withdraw(enable).await?;
        let value = Self::depool_fee(config)?;
        Ok(Self {m, depool, value, body, config, with_answer: true})
    }

    pub async fn replenish(
        m: &'a ArgMatches<'_>,
        config: &'a Config,
        depool: &'a str
    ) -> Result<DepoolCmd<'a>, String> {
        let value = parse_value(m)?;
        let body = encode_replenish_stake().await?;
        Ok(Self {m, depool, value, body, config, with_answer: false})
    }

    pub async fn ticktock(
        m: &'a ArgMatches<'_>,
        config: &'a Config,
        depool: &'a str
    ) -> Result<DepoolCmd<'a>, String> {
        let value = 1000000000;
        let body = encode_ticktock().await?;
        Ok(Self {m, depool, value, body, config, with_answer: false})
    }

    pub async fn execute(mut self) -> Result<(), String> {
        let body = std::mem::take(&mut self.body);
        let call_args = CallArgs::submit_with_args(self.m, &self.depool, &format!("{}", self.value), true, body).await?;
        let msig_args = MultisigArgs::new(self.m, self.config, call_args)?;
        
        let since = now();
        let depool = self.depool.to_owned();
        let wallet = msig_args.address().to_owned();

        let result = msig_args.execute(self.config).await?;
        if self.config.no_answer {
            if !self.config.is_json {
                println!("Succeeded.");
            }
            return call::print_json_result(result, self.config);
        }
        println!("\nMessage was successfully sent to the multisig, waiting for message to be sent to the depool...");

        let client = create_client(self.config)?;
        let message = ton_client::net::wait_for_collection(
            client.clone(),
            ParamsOfWaitForCollection {
                collection: "messages".to_owned(),
                filter: Some(answer_filter(&wallet, &depool, since)),
                result: "id body created_at created_at_string".to_owned(),
                timeout: Some(self.config.timeout),
                ..Default::default()
            },
        ).await.map_err(|e| println!("failed to query message: {}", e));

        if message.is_err() {
            println!("Request failed. Check the contract balance to be great enough to cover transfer value with possible fees.");
            return Ok(());
        }
        println!("\nRequest was successfully sent to depool.");
        if self.with_answer {
            println!("\nWaiting for depool answer...");

            let mut statuses: HashMap<u32, &str> = HashMap::new();
            statuses.insert(0, "SUCCESS");
            statuses.insert(1, "STAKE_TOO_SMALL");
            statuses.insert(3, "DEPOOL_CLOSED");
            statuses.insert(6, "NO_PARTICIPANT");
            statuses.insert(9, "PARTICIPANT_ALREADY_HAS_VESTING");
            statuses.insert(10, "WITHDRAWAL_PERIOD_GREATER_TOTAL_PERIOD");
            statuses.insert(11, "TOTAL_PERIOD_MORE_18YEARS");
            statuses.insert(12, "WITHDRAWAL_PERIOD_IS_ZERO");
            statuses.insert(13, "TOTAL_PERIOD_IS_NOT_DIVISIBLE_BY_WITHDRAWAL_PERIOD");
            statuses.insert(16, "REMAINING_STAKE_LESS_THAN_MINIMAL");
            statuses.insert(17, "PARTICIPANT_ALREADY_HAS_LOCK");
            statuses.insert(18, "TRANSFER_AMOUNT_IS_TOO_BIG");
            statuses.insert(19, "TRANSFER_SELF");
            statuses.insert(20, "TRANSFER_TO_OR_FROM_VALIDATOR");
            statuses.insert(21, "FEE_TOO_SMALL");
            statuses.insert(22, "INVALID_ADDRESS");
            statuses.insert(23, "INVALID_DONOR");
            statuses.insert(24, "NO_ELECTION_ROUND");
            statuses.insert(25, "INVALID_ELECTION_ID");
            statuses.insert(26, "TRANSFER_WHILE_COMPLETING_STEP");
            statuses.insert(27, "NO_POOLING_STAKE");

            let message = ton_client::net::wait_for_collection(
                client.clone(),
                ParamsOfWaitForCollection {
                    collection: "messages".to_owned(),
                    filter: Some(answer_filter(&depool, &wallet, since)),
                    result: "id body created_at created_at_string value".to_owned(),
                    timeout: Some(self.config.timeout),
                    ..Default::default()
                },
            ).await.map_err(|e| println!("failed to query answer: {}", e));
            if message.is_ok() {
                let message = message.unwrap().result;
                println!("\nAnswer: ");
                let (name, args) = print_message(client.clone(), &message, PARTICIPANT_ABI, true).await?;
                if name == "receiveAnswer" {
                    let args: serde_json::Value = serde_json::from_str(&args)
                        .map_err(|e| format!("failed to deserialize args: {}", e))?;
                    let status = args["errcode"].as_str()
                        .ok_or("failed to serialize the error code")?
                        .parse::<u32>()
                        .map_err(|e| format!("failed to parse the error code: {}", e))?;
                    let comment = args["comment"].as_str()
                        .ok_or("failed to serialize the comment")?;
                    if statuses.contains_key(&status) {
                        println!("Answer status: {}\nComment: {}", statuses[&status], comment);
                    } else {
                        println!("Answer status: Unknown({})\nComment: {}", status, comment);
                    }
                }
                println!();
            } else {
                println!("\nThere were no answer messages during the timeout period.\n");
            }
        }
        println!("Done");
        Ok(())
    }

    fn depool_fee(config: &Config) -> Result<u64, String> {
        let depool_fee = config.depool_fee.clone().to_string();
        u64::from_str_radix(&convert::convert_token(&depool_fee)?, 10)
            .map_err(|e| format!(r#"failed to parse depool fee value: {}"#, e))
    }
}

fn parse_value(m:  &ArgMatches<'_>) -> Result<u64, String> {
    let amount = m.value_of("VALUE")
        .ok_or("value is not defined.".to_string())?;
    let amount = u64::from_str_radix(&convert::convert_token(amount)?, 10)
        .map_err(|e| format!(r#"failed to parse stake value: {}"#, e))?;
    Ok(amount)
}

pub async fn depool_command(m: &ArgMatches<'_>, config: &mut Config) -> Result<(), String> {
    let depool = m.value_of("ADDRESS")
        .map(|s| s.to_owned())
        .or(config.addr.clone())
        .ok_or("depool address is not defined. Supply it in the config file or in command line.".to_string())?;
    let depool = load_ton_address(&depool, config)
        .map_err(|e| format!("invalid depool address: {}", e))?;

    let mut set_wait_answer = |m: &ArgMatches| {
        if m.is_present("WAIT_ANSWER") {
            config.no_answer = false;
        }
    };
    set_wait_answer(m);
    if let Some(m) = m.subcommand_matches("donor") {
        let matches = m.subcommand_matches("vesting").or(m.subcommand_matches("lock"));
        if let Some(matches) = matches {
            let is_vesting = m.subcommand_matches("vesting").is_some();
            set_wait_answer(matches);
            return DepoolCmd::donor(matches, config, &depool, is_vesting).await?.execute().await;
        }
    }

    if let Some(m) = m.subcommand_matches("stake") {
        if let Some(m) = m.subcommand_matches("ordinary") {
            set_wait_answer(m);
            return DepoolCmd::stake_ordinary(m, config, &depool).await?.execute().await;
        }
        if let Some(m) = m.subcommand_matches("vesting") {
            set_wait_answer(m);
            return DepoolCmd::stake_vesting(m, config, &depool, false).await?.execute().await;
        }
        if let Some(m) = m.subcommand_matches("lock") {
            set_wait_answer(m);
            return DepoolCmd::stake_vesting(m, config, &depool, true).await?.execute().await;
        }
        if let Some(m) = m.subcommand_matches("remove") {
            set_wait_answer(m);
            return DepoolCmd::stake_remove(m, config, &depool).await?.execute().await;
        }
        if let Some(m) = m.subcommand_matches("withdrawPart") {
            set_wait_answer(m);
            return DepoolCmd::stake_withdraw_part(m, config, &depool).await?.execute().await;
        }
        if let Some(m) = m.subcommand_matches("transfer") {
            set_wait_answer(m);
            return DepoolCmd::stake_transfer(m, config, &depool).await?.execute().await;
        }
    }
    if let Some(m) = m.subcommand_matches("withdraw") {
        let matches = m.subcommand_matches("on")
            .or(m.subcommand_matches("off"));
        let enable_withdraw = m.subcommand_matches("on").is_some();
        if let Some(m) = matches {
            set_wait_answer(m);
            return DepoolCmd::withdraw(m, config, &depool, enable_withdraw).await?.execute().await;
        }
    }
    if let Some(m) = m.subcommand_matches("events") {
        return events_command(m, &config, &depool).await
    }
    if let Some(m) = m.subcommand_matches("answers") {
        return answer_command(m, &config, &depool).await
    }
    if let Some(m) = m.subcommand_matches("replenish") {
        return DepoolCmd::replenish(m, config, &depool).await?.execute().await;
    }
    if let Some(m) = m.subcommand_matches("ticktock") {
        return DepoolCmd::ticktock(m, config, &depool).await?.execute().await;
    }
    Err("unknown depool command".to_owned())
}

async fn answer_command(m: &ArgMatches<'_>, config: &Config, depool: &str) -> Result<(), String> {
    let wallet = m.value_of("MSIG")
        .map(|s| s.to_string())
        .or(config.wallet.clone())
        .ok_or("multisig wallet address is not defined.".to_string())?;
    let since = m.value_of("SINCE")
        .map(|s| {
            u32::from_str_radix(s, 10)
            .map_err(|e| format!(r#"cannot parse "since" option: {}"#, e))
        })
        .transpose()?
        .unwrap_or(0);

    let ton = create_client_verbose(config)?;
    let wallet = load_ton_address(&wallet, config)
        .map_err(|e| format!("invalid depool address: {}", e))?;

    let messages = ton_client::net::query_collection(
        ton.clone(),
        ParamsOfQueryCollection {
            collection: "messages".to_owned(),
            filter: Some(answer_filter(depool, &wallet, since)),
            result: "id value body created_at created_at_string".to_owned(),
            order: Some(vec![OrderBy{ path: "created_at".to_owned(), direction: SortDirection::DESC }]),
            ..Default::default()
        },
    ).await.map_err(|e| format!("failed to query depool messages: {}", e))?;
    println!("{} answers found", messages.result.len());
    for messages in &messages.result {
        print_answer(ton.clone(), messages).await?;
    }
    println!("Done");
    Ok(())
}

async fn print_answer(ton: TonClient, message: &serde_json::Value) -> Result <(), String> {
    println!("Answer:");
    print_message(ton, message, PARTICIPANT_ABI, true).await?;
    Ok(())
}

/*
 * Events command
 */

async fn events_command(m: &ArgMatches<'_>, config: &Config, depool: &str) -> Result<(), String> {
    let since = m.value_of("SINCE");
    let wait_for = m.is_present("WAITONE");
    let depool = Some(depool);
    print_args!(depool, since);
    if !wait_for {
        let since = since.map(|s| {
                u32::from_str_radix(s, 10)
                    .map_err(|e| format!(r#"cannot parse "since" option: {}"#, e))
            })
            .transpose()?
            .unwrap_or(0);
        get_events(config, depool.unwrap(), since).await
    } else {
        wait_for_event(config, depool.unwrap()).await
    }
}

async fn print_event(ton: TonClient, event: &serde_json::Value) -> Result<(), String> {
    println!("event {}", event["id"].as_str()
        .ok_or("failed to serialize event id")?);

    let body = event["body"].as_str()
        .ok_or("failed to serialize event body")?;
    let def_config = Config::default();
    let result = ton_client::abi::decode_message_body(
        ton.clone(),
        ParamsOfDecodeMessageBody {
            abi: load_abi(DEPOOL_ABI, &def_config).await.map_err(|e| format!("failed to load depool abi: {}", e))?,
            body: body.to_owned(),
            is_internal: false,
            ..Default::default()
        },
    );
    let (name, args) = if result.is_err() {
        ("unknown".to_owned(), "{}".to_owned())
    } else {
        let result = result.unwrap();
        (result.name, serde_json::to_string(&result.value)
            .map_err(|e| format!("failed to serialize the result: {}", e))?)
    };

    println!("{} {} ({})\n{}\n",
        name,
        event["created_at"].as_u64().ok_or("failed to serialize event field")?,
        event["created_at_string"].as_str().ok_or("failed to serialize event field")?,
        args
    );
    Ok(())
}

async fn get_events(config: &Config, depool: &str, since: u32) -> Result<(), String> {
    let ton = create_client_verbose(&config)?;
    let _addr = load_ton_address(depool, &config)?;

    let events = ton_client::net::query_collection(
        ton.clone(),
        ParamsOfQueryCollection {
            collection: "messages".to_owned(),
            filter: Some(events_filter(depool, since)),
            result: "id body created_at created_at_string".to_owned(),
            order: Some(vec![OrderBy{ path: "created_at".to_owned(), direction: SortDirection::DESC }]),
            ..Default::default()
        },
    ).await.map_err(|e| format!("failed to query depool events: {}", e))?;
    println!("{} events found", events.result.len());
    for event in &events.result {
        print_event(ton.clone(), event).await?;
    }
    println!("Done");
    Ok(())
}

async fn wait_for_event(config: &Config, depool: &str) -> Result<(), String> {
    let ton = create_client_verbose(&config)?;
    let _addr = load_ton_address(depool, &config)?;
    println!("Waiting for a new event...");
    let event = ton_client::net::wait_for_collection(
        ton.clone(),
        ParamsOfWaitForCollection {
            collection: "messages".to_owned(),
            filter: Some(events_filter(depool, now())),
            result: "id body created_at created_at_string".to_owned(),
            timeout: Some(config.timeout),
            ..Default::default()
        },

    ).await.map_err(|e| println!("failed to query event: {}", e));
    if event.is_ok() {
        print_event(ton.clone(), &event.unwrap().result).await?;
    }
    Ok(())
}

async fn encode_body(func: &str, params: serde_json::Value) -> Result<String, String> {
    let client = create_client_local()?;
    let def_config = Config::default();
    ton_client::abi::encode_message_body(
        client.clone(),
        ParamsOfEncodeMessageBody {
            abi: load_abi(DEPOOL_ABI, &def_config).await?,
            call_set: CallSet::some_with_function_and_input(func, params)
                .ok_or("failed to create CallSet with specified parameters.")?,
            is_internal: true,
            ..Default::default()
        },
    ).await
    .map_err(|e| format!("failed to encode body: {}", e))
    .map(|r| r.body)
}

async fn encode_set_withdraw(flag: bool) -> Result<String, String> {
    if flag {
        encode_body("withdrawAll", json!({}))
    } else {
        encode_body("cancelWithdrawal", json!({}))
    }.await
}

async fn encode_add_ordinary_stake(stake: u64) -> Result<String, String> {
	encode_body("addOrdinaryStake", json!({
        "stake": stake
    })).await
}

async fn encode_replenish_stake() -> Result<String, String> {
	encode_body("receiveFunds", json!({})).await
}

async fn encode_ticktock() -> Result<String, String> {
	encode_body("ticktock", json!({})).await
}

async fn encode_add_vesting_stake(
    stake: u64,
    beneficiary: &str,
    tperiod: u32,
    wperiod: u32,
) -> Result<String, String> {
	encode_body("addVestingStake", json!({
        "stake": stake,
        "beneficiary": beneficiary,
        "withdrawalPeriod": wperiod,
        "totalPeriod": tperiod
    })).await
}

async fn encode_set_donor(is_vesting: bool, donor: &str) -> Result<String, String> {
    if is_vesting {
        encode_body("setVestingDonor", json!({
            "donor": donor
        }))
    } else {
        encode_body("setLockDonor", json!({
            "donor": donor
        }))
    }.await
}

async fn encode_add_lock_stake(
    stake: u64,
    beneficiary: &str,
    tperiod: u32,
    wperiod: u32,
) -> Result<String, String> {
	encode_body("addLockStake", json!({
        "stake": stake,
        "beneficiary": beneficiary,
        "withdrawalPeriod": wperiod,
        "totalPeriod": tperiod
    })).await
}

async fn encode_remove_stake(target_value: u64) -> Result<String, String> {
	encode_body("withdrawFromPoolingRound", json!({
        "withdrawValue": target_value
    })).await
}

async fn encode_withdraw_stake(target_value: u64) -> Result<String, String> {
	encode_body("withdrawPart", json!({
        "withdrawValue": target_value
    })).await
}

async fn encode_transfer_stake(dest: &str, amount: u64) -> Result<String, String> {
	encode_body("transferStake", json!({
        "dest": dest,
        "amount": amount
    })).await
}