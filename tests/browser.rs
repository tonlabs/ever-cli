use assert_cmd::Command;
use predicates::prelude::*;
// uncomment for debug
// use std::io::Write;
use serde_json::json;
mod common;
use common::{BIN_NAME, NETWORK, giver, grep_address};

fn get_debot_paths(name: &str) -> (String, String, String) {
    (
        format!("tests/samples/{}.tvc", name),
        format!("tests/samples/{}.abi.json", name),
        format!("tests/{}.keys.json", name),
    )
}

fn deploy_debot(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let (tvc, abi, keys) = get_debot_paths(name);

    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.arg("config")
        .arg("--url")
        .arg(&*NETWORK)
        .arg("--wc")
        .arg("0");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    let out = cmd
        .arg("genaddr")
        .arg(&tvc)
        .arg(&abi)
        .arg("--genkey")
        .arg(&keys)
        .output()
        .expect("Failed to generate address.");
    let addr = grep_address(&out.stdout);
    giver(&addr);

    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.arg("deploy")
        .arg(&tvc)
        .arg("{}")
        .arg("--abi")
        .arg(&abi)
        .arg("--sign")
        .arg(&keys);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(&addr))
        .stdout(predicate::str::contains("Transaction succeeded."));

    let abi_string = std::fs::read_to_string(&abi).unwrap();
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.arg("call")
        .arg("--abi")
        .arg(&abi)
        .arg("--sign")
        .arg(&keys)
        .arg(&addr)
        .arg("setABI")
        .arg(format!(r#"{{"dabi":"{}"}}"#, hex::encode(abi_string)));
    cmd.assert()
        .success();

    Ok(addr)
}

#[test]
fn test_signing_box_interface() -> Result<(), Box<dyn std::error::Error>> {
    let addr = deploy_debot("sample1")?;
    let (_, _, keys) = get_debot_paths("sample1");

    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .write_stdin(format!("y\n{}", keys))
        .arg("debot")
        .arg("fetch")
        .arg(&addr);
    let _cmd = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Enter my signing keys:"))
        .stdout(predicate::str::contains("Signing Box Handle:"))
        .stdout(predicate::str::contains("test sign hash passed"));
    // uncomment for debug
    // let out = cmd.get_output();
    // std::io::stdout().lock().write_all(&out.stdout)?;
    Ok(())
}

#[test]
fn test_userinfo() -> Result<(), Box<dyn std::error::Error>> {
    let addr = deploy_debot("sample2")?;
    let (_, abi, keys) = get_debot_paths("sample2");
    let wallet = format!("0:{:064}", 1);
    let key = format!("0x{:064}", 2);
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.arg("config")
        .arg("--wallet")
        .arg(&wallet)
        .arg("--pubkey")
        .arg(&key);
    cmd.assert()
        .success();

    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.arg("call")
        .arg("--abi")
        .arg(abi)
        .arg("--sign")
        .arg(keys)
        .arg(&addr)
        .arg("setParams")
        .arg(format!(r#"{{"wallet":"{}","key":"{}"}}"#, wallet, key));
    cmd.assert()
        .success();

    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .write_stdin(format!("y\n"))
        .arg("debot")
        .arg("start")
        .arg(&addr);
    let _cmd = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Account is valid"))
        .stdout(predicate::str::contains("Public key is valid"));
    Ok(())
}

#[test]
fn test_pipechain_inputs() -> Result<(), Box<dyn std::error::Error>> {
    let path_to_pipechain = "tests/PipechainTest1.chain";
    let path_to_pipechain_tmp = "tests/PipechainTest1.chain_tmp";
    let addr = deploy_debot("PipechainTest")?;
    let (_, _, _) = get_debot_paths("PipechainTest");
    let chain = std::fs::read_to_string(path_to_pipechain)?;
    let mut val: serde_json::Value = serde_json::from_str(&chain)?;
    val["debotAddress"] = json!(addr);
    let return_value = val["initArgs"]["arg7"].clone();
    std::fs::write(path_to_pipechain_tmp, serde_json::to_string_pretty(&val).unwrap())?;

    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .arg("-j")
        .arg("debot")
        .arg("start")
        .arg(&addr)
        .arg("--pipechain")
        .arg(path_to_pipechain_tmp);
    let assert = cmd
        .assert()
        .success();

    let _ = std::fs::remove_file(path_to_pipechain_tmp)?;

    let out_value: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    let eq = predicate::eq(return_value);
    assert_eq!(true,  eq.eval(&out_value["ret1"]));
    // uncomment for debug
    // let out = cmd.get_output();
    // std::io::stdout().lock().write_all(&out.stdout)?;
    Ok(())
}

#[test]
fn test_encryptionboxes() -> Result<(), Box<dyn std::error::Error>> {
    let addr = deploy_debot("sample3")?;
    let (_, _, keys) = get_debot_paths("sample3");
    
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .write_stdin(format!("y\n{}\n{}\n{}", keys, keys, keys))
        .arg("debot")
        .arg("fetch")
        .arg(&addr);
    let _cmd = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("run EncryptionBoxInput"))
        .stdout(predicate::str::contains("run chacha"))
        .stdout(predicate::str::contains("ChaCha works"))
        .stdout(predicate::str::contains("run secret naclbox"))
        .stdout(predicate::str::contains("SecretNaCl works"))
        .stdout(predicate::str::contains("run naclbox"))
        .stdout(predicate::str::contains("NaCl works"));
    // uncomment for debug 
    // let out = cmd.get_output();
    // std::io::stdout().lock().write_all(&out.stdout)?;
    Ok(())
}

#[test]
fn test_address_input() -> Result<(), Box<dyn std::error::Error>> {
    let addr = deploy_debot("AddressInput")?;
    
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .write_stdin(format!("y\n{}", "0:ea5be6a13f20fcdfddc2c2b0d317dfeab56718249b090767e5940137b7af89f1"))
        .arg("debot")
        .arg("fetch")
        .arg(&addr);
    let _cmd = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Enter Address:"))
        .stdout(predicate::str::contains("AddressInput tests completed!"));
    Ok(())
}

#[test]
fn test_amount_input() -> Result<(), Box<dyn std::error::Error>> {
    let addr = deploy_debot("AmountInput")?;
    
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .write_stdin(format!("y\n{}", "99.456654321"))
        .arg("debot")
        .arg("fetch")
        .arg(&addr);
    let _cmd = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Enter amount of tons with decimals:"))
        .stdout(predicate::str::contains("(>= 0.000000000 and <= 100.000000000)"))
        .stdout(predicate::str::contains("AmountInput tests completed!"));
    Ok(())
}

#[test]
fn test_confirm_input() -> Result<(), Box<dyn std::error::Error>> {
    let addr = deploy_debot("ConfirmInput")?;
    
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .write_stdin(format!("y\n{}", "y"))
        .arg("debot")
        .arg("fetch")
        .arg(&addr);
    let _cmd = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Select: (y/n)"))
        .stdout(predicate::str::contains("ConfirmInput tests completed!"));
    Ok(())
}

#[test]
fn test_number_input() -> Result<(), Box<dyn std::error::Error>> {
    let addr = deploy_debot("NumberInput")?;
    
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .write_stdin(format!("y\n{}", "79"))
        .arg("debot")
        .arg("fetch")
        .arg(&addr);
    let _cmd = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Enter number:"))
        .stdout(predicate::str::contains("NumberInput tests completed!"));
    Ok(())
}

#[test]
fn test_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let addr = deploy_debot("Terminal")?;
    
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .write_stdin(format!("y\n{}", "Test value"))
        .arg("debot")
        .arg("fetch")
        .arg(&addr);
    let _cmd = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Enter text"))
        .stdout(predicate::str::contains("Terminal tests completed!"));
    Ok(())
}

#[test]
fn test_country_input() -> Result<(), Box<dyn std::error::Error>> {
    let addr = deploy_debot("CountryInput")?;
    
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .write_stdin(format!("y\n{}\n{}", "RU", "ES"))
        .arg("debot")
        .arg("fetch")
        .arg(&addr);
    let _cmd = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Select a country:"))
        .stdout(predicate::str::contains("These country codes are permitted: RU GB KZ"))
        .stdout(predicate::str::contains("Country test for permitted list completed"))
        .stdout(predicate::str::contains("Select a country:"))
        .stdout(predicate::str::contains("These country codes are banned: RU GB KZ"))
        .stdout(predicate::str::contains("Country test for banned list completed"));
    Ok(())
}

#[test]
fn test_datetime_input() -> Result<(), Box<dyn std::error::Error>> {
    let addr = deploy_debot("DateTimeInput")?;
    
    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd.timeout(std::time::Duration::from_secs(2))
        .write_stdin(format!("y\n{}\n{}\n{}", "1234568800", "60000", "1234567890"))
        .arg("debot")
        .arg("fetch")
        .arg(&addr);
    let _cmd = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains(""))
        .stdout(predicate::str::contains(""))
        .stdout(predicate::str::contains(""))
        .stdout(predicate::str::contains(""))
        .stdout(predicate::str::contains(""))
        .stdout(predicate::str::contains(""))
        .stdout(predicate::str::contains(""))
        .stdout(predicate::str::contains(""))
        .stdout(predicate::str::contains(""))
        .stdout(predicate::str::contains(""))
        .stdout(predicate::str::contains(""));
    Ok(())
}

#[test]
fn test_pipechain_signing() -> Result<(), Box<dyn std::error::Error>> {
    let path_to_pipechain = "tests/PipechainTest2.chain";
    let addr = deploy_debot("PipechainTest")?;
    let (_, _, keys) = get_debot_paths("PipechainTest");
    let chain = std::fs::read_to_string(path_to_pipechain)?;
    let mut val: serde_json::Value = serde_json::from_str(&chain)?;
    val["debotAddress"] = json!(addr);

    let mut cmd = Command::cargo_bin(BIN_NAME)?;
    cmd
        .arg("-j")
        .arg("debot")
        .arg("start")
        .arg(&addr)
        .arg("--pipechain")
        .arg(path_to_pipechain)
        .arg("--signkey")
        .arg(keys);
    let _assert = cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Debot error").count(0));
    Ok(())
}
