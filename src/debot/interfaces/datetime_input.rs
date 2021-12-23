use crate::debot::term_browser::terminal_input;
use serde_json::Value;
use chrono::prelude::NaiveDateTime;
use chrono::offset::{Offset};
use chrono::Local;
use std::convert::TryFrom;
use ton_client::abi::Abi;
use ton_client::debot::{DebotInterface, InterfaceResult};
use super::dinterface::{decode_answer_id, decode_prompt, decode_num_arg};

const ID: &'static str = "4e862a9df81183ab425bdf0fbd76bd0b558c7f44c24887b4354bf1c26c74a623";

pub const ABI: &str = r#"
{
    "ABI version": 2,
    "version": "2.2",
    "header": ["time"],
    "functions": [
        {
            "name": "getDate",
            "inputs": [
                {"name":"answerId","type":"uint32"},
                {"name":"prompt","type":"string"},
                {"name":"defaultDate","type":"int128"},
                {"name":"minDate","type":"int128"},
                {"name":"maxDate","type":"int128"}
            ],
            "outputs": [
                {"name":"date","type":"int128"}
            ]
        },
        {
            "name": "getTime",
            "inputs": [
                {"name":"answerId","type":"uint32"},
                {"name":"prompt","type":"string"},
                {"name":"defaultTime","type":"uint32"},
                {"name":"minTime","type":"uint32"},
                {"name":"maxTime","type":"uint32"},
                {"name":"minuteInterval","type":"uint8"}
            ],
            "outputs": [
                {"name":"time","type":"uint32"}
            ]
        },
        {
            "name": "getDateTime",
            "inputs": [
                {"name":"answerId","type":"uint32"},
                {"name":"prompt","type":"string"},
                {"name":"defaultDatetime","type":"int128"},
                {"name":"minDatetime","type":"int128"},
                {"name":"maxDatetime","type":"int128"},
                {"name":"minuteInterval","type":"uint8"},
                {"name":"inTimeZoneOffset","type":"int16"}
            ],
            "outputs": [
                {"name":"datetime","type":"int128"},
                {"name":"timeZoneOffset","type":"int16"}
            ]
        },
        {
            "name": "getTimeZoneOffset",
            "inputs": [
                {"name":"answerId","type":"uint32"}
            ],
            "outputs": [
                {"name":"timeZoneOffset","type":"int16"}
            ]
        },
        {
            "name": "constructor",
            "inputs": [
            ],
            "outputs": [
            ]
        }
    ],
    "data": [
    ],
    "events": [
    ],
    "fields": [
        {"name":"_pubkey","type":"uint256"},
        {"name":"_timestamp","type":"uint64"},
        {"name":"_constructorFlag","type":"bool"}
    ]
}
"#;

pub struct DateTimeInput {}

impl DateTimeInput {
    fn print_date(stamp : i128)->Result<String, String> {
        let dt = NaiveDateTime::from_timestamp(i64::try_from(stamp).map_err(|_e| "failed to transform i128 to i64")?, 0).date();
        Ok(dt.format("%d.%m.%y").to_string())
    }

    fn parse_date(date : &String)->Result<i128, String> {
        let parse_from_str = NaiveDateTime::parse_from_str;
        let datetime_repr = format!("{} 00:00:00", date);
        Ok(
            i128::try_from(
                parse_from_str(
                    &datetime_repr.as_str(),
                    "%d.%m.%y %H:%M:%S"
                ).map_err(|_e| "failed to parse datetime")?
                .timestamp()
            ).map_err(|_e| "failed to transform i64 to i128")?
        )
    }

    fn print_time(stamp: u32)->Result<String, String> {
        let tm = NaiveDateTime::from_timestamp(i64::try_from(stamp).map_err(|_e| "failed to transform u32 to i64")?, 0).time();
        Ok(tm.format("%H:%M:%S").to_string())
    }

    fn parse_time(time : &String)->Result<u32, String> {
        let parse_from_str = NaiveDateTime::parse_from_str;
        let datetime_repr = format!("01.01.1970 {}", time);
        Ok(
            u32::try_from(
                parse_from_str(
                    &datetime_repr.as_str(),
                    "%d.%m.%Y %H:%M:%S"
                ).map_err(|_e| "failed to parse datetime")?
                .timestamp()
            ).map_err(|_e| "failed to transform i64 to u32")?
        )
    }

    fn print_datetime(stamp : i128)->Result<String, String> {
        let dtm = NaiveDateTime::from_timestamp(i64::try_from(stamp).map_err(|_e| "failed to transform i128 to i64")?, 0);
        Ok(dtm.format("%d.%m.%y %H:%M:%S").to_string())
    }

    fn parse_datetime(datetime : &String)->Result<i128, String> {
        let parse_from_str = NaiveDateTime::parse_from_str;
        Ok(
            i128::try_from(
                parse_from_str(
                    datetime.as_str(),
                    "%d.%m.%Y %H:%M:%S"
                ).map_err(|_e| "failed to parse datetime")?
                .timestamp()
            ).map_err(|_e| "failed to transform i64 to i128")?
        )
    }

    pub fn new() -> Self {
        Self {}
    }
    fn get_date(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let prompt = decode_prompt(args)?;
        let default_date = decode_num_arg::<i128>(args, "defaultDate")?;
        let min_date = decode_num_arg::<i128>(args, "minDate")?;
        let max_date = decode_num_arg::<i128>(args, "maxDate")?;
        let mut value = String::new();
        let prompt = format!(
            "{}\nSpecify date following format *dd.mm.yy* or press Enter to proceed with default value {}\n(>= {} and <= {})",
            prompt,
            DateTimeInput::print_date(default_date)?,
            DateTimeInput::print_date(min_date)?,
            DateTimeInput::print_date(max_date)?);
        let _ = terminal_input(&prompt, |val| {
            let number;
            if val == "" {
                number = default_date;
            } else {
                number = DateTimeInput::parse_date(val)?;
            }

            if number < min_date || number > max_date {
                return Err(format!("date is out of range"));
            }
            value = number.to_string();
            Ok(())
        });
        Ok((answer_id, json!({"time": value})))
    }
    fn get_time(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let prompt = decode_prompt(args)?;
        let default_time = decode_num_arg::<u32>(args, "defaultTime")?;
        let min_time = decode_num_arg::<u32>(args, "minTime")?;
        let max_time = decode_num_arg::<u32>(args, "maxTime")?;
        let _minute_interval = decode_num_arg::<u8>(args, "minuteInterval")?;
        let mut time : u32 = 1;
        let prompt = format!(
            "{}\nSpecify time following format *HH:MM:SS* or press Enter to proceed with default value {}\n(>= {} and <= {})",
            prompt,
            DateTimeInput::print_time(default_time)?,
            DateTimeInput::print_time(min_time)?,
            DateTimeInput::print_time(max_time)?);
        let _ = terminal_input(&prompt, |val| {
            if val == "" {
                time = default_time;
            } else {
                time = DateTimeInput::parse_time(val)?;
            }

            if time < min_time || time > max_time {
                return Err(format!("time is out of range"));
            }
            Ok(())
        });
        Ok((answer_id, json!({"time": time})))
    }
    fn get_date_time(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let prompt = decode_prompt(args)?;
        let in_time_zone_offset = decode_num_arg::<i16>(args, "inTimeZoneOffset")?;
        let time_zone_offset = i16::try_from(Local::now().offset().fix().local_minus_utc() / 60)
            .map_err(|_e| "failed to convert i32 to i16")?;
        let diff_by_timezone : i128 = (60 * (time_zone_offset - in_time_zone_offset)).into();
        let default_datetime = decode_num_arg::<i128>(args, "defaultDatetime")? + diff_by_timezone;
        let min_datetime = decode_num_arg::<i128>(args, "minDatetime")? + diff_by_timezone;
        let max_datetime = decode_num_arg::<i128>(args, "maxDatetime")? + diff_by_timezone;
        let _minute_interval = decode_num_arg::<u8>(args, "minuteInterval")?;
        let prompt = format!(
            "{}\nSpecify date and time following format *dd.mm.yy HH:MM:SS* or press Enter to proceed with default value {}\n(>= {} and <= {})",
            prompt,
            DateTimeInput::print_datetime(default_datetime)?,
            DateTimeInput::print_datetime(min_datetime)?,
            DateTimeInput::print_datetime(max_datetime)?);
        let mut datetime : i128 = 1;
        let _ = terminal_input(&prompt, |val| {
            if val == "" {
                datetime = default_datetime;
            } else {
                datetime = DateTimeInput::parse_datetime(val)?;
            }

            if datetime < min_datetime || datetime > max_datetime {
                return Err(format!("datetime is out of range"));
            }
            Ok(())
        });
        Ok((answer_id, json!({"datetime": datetime, "timeZoneOffset": time_zone_offset})))
    }
    fn get_time_zone_offset(&self, args: &Value) -> InterfaceResult {
        let answer_id = decode_answer_id(args)?;
        let time_zone_offset = i16::try_from(Local::now().offset().fix().local_minus_utc() / 60)
            .map_err(|_e| "failed to convert i32 to i16")?;
        Ok((answer_id, json!({"timeZoneOffset": time_zone_offset})))
    }
}

#[async_trait::async_trait]
impl DebotInterface for DateTimeInput {
    fn get_id(&self) -> String {
        ID.to_string()
    }

    fn get_abi(&self) -> Abi {
        Abi::Json(ABI.to_owned())
    }

    async fn call(&self, func: &str, args: &Value) -> InterfaceResult {
        match func {
            "getDate" => self.get_date(args),
            "getTime" => self.get_time(args),
            "getDateTime" => self.get_date_time(args),
            "getTimeZoneOffset" => self.get_time_zone_offset(args),
            _ => Err(format!("function \"{}\" is not implemented", func)),
        }
    }
}
