use ton_api::{
    ton::{
        self, TLObject,
        accountaddress::AccountAddress,
        raw::ShardAccountState
    }
};
use ton_block::{
    AccountStatus, Deserializable, ShardAccount
};

use crate::client::SendReceive;
use crate::client::downcast;
use ton_types::write_boc;
use ton_types::Result;

pub(crate) struct GetAccount {
    pub account: AccountAddress
}

impl SendReceive for GetAccount {
    fn send(&self) -> Result<TLObject> {
        Ok(TLObject::new(ton::rpc::raw::GetShardAccountState {account_address: self.account.clone()}))
    }

    fn receive(&self, answer: TLObject) -> Result<(String, Vec<u8>)> {
        let shard_account_state = downcast::<ShardAccountState>(answer)?;
        let mut account_info = String::from("{");
        account_info.push_str("\n\"");
        account_info.push_str("acc_type_name\":\t\"");

        match shard_account_state {
            ShardAccountState::Raw_ShardAccountNone => {
                account_info.push_str(&"Nonexist");
            },
            ShardAccountState::Raw_ShardAccountState(account_state) => {
                let shard_account = ShardAccount::construct_from_bytes(&account_state.shard_account)?;
                let account = shard_account.read_account()?;

                let account_type = match account.status() {
                    AccountStatus::AccStateUninit => "Uninit",
                    AccountStatus::AccStateFrozen => "Frozen",
                    AccountStatus::AccStateActive => "Active",
                    AccountStatus::AccStateNonexist => "Nonexist"
                };
                let balance = account.balance().map_or(0, |val| val.grams.as_u128());
                account_info.push_str(&account_type);
                account_info.push_str("\",\n\"");
                account.get_addr().map_or_else(||{}, |v| {
                    account_info.push_str("id\":\t\"");
                    account_info.push_str(&v.to_string());
                    account_info.push_str("\",\n\"");
                });
                account.get_code_hash().map_or_else(||{}, |v| {
                    account_info.push_str("code_hash\":\t\"");
                    account_info.push_str(&v.as_hex_string());
                    account_info.push_str("\",\n\"");
                });
                account_info.push_str("balance\":\t\"");
                account_info.push_str(&balance.to_string());
                account_info.push_str("\",\n\"");
                account_info.push_str("last_paid\":\t");
                account_info.push_str(&account.last_paid().to_string());
                account_info.push_str(",\n\"");
                account_info.push_str("last_trans_lt\":\t\"");
                account_info.push_str(&format!("{:#x}", shard_account.last_trans_lt()));
                account_info.push_str("\",\n\"");
                account_info.push_str("data\":\t\"");
                account_info.push_str(
                    &base64::encode(&write_boc(&shard_account.account_cell())?)
                );
            }
        }
        account_info.push_str("\"\n}");
        let account_data = account_info.as_bytes().to_vec();

        Ok((account_info, account_data))
    }
}
