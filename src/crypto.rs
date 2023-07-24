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
use crate::config::{WORD_COUNT, HD_PATH};
use crate::helpers::{create_client_local, read_keys, check_dir};
use ton_client::crypto::{
    KeyPair,
    mnemonic_from_random,
    hdkey_xprv_from_mnemonic,
    hdkey_secret_from_xprv,
    nacl_sign_keypair_from_secret_key,
    hdkey_derive_from_xprv_path,
    MnemonicDictionary,
    ParamsOfHDKeySecretFromXPrv,
    ParamsOfHDKeyDeriveFromXPrvPath,
    ParamsOfHDKeyXPrvFromMnemonic,
    ParamsOfNaclSignKeyPairFromSecret,
    ParamsOfMnemonicFromRandom
};
use crate::Config;

pub fn load_keypair(keys: &str) -> Result<KeyPair, String> {
    if keys.find(' ').is_none() {
        let keys = read_keys(keys)?;
        Ok(keys)
    } else {
        generate_keypair_from_mnemonic(keys)
    }
}

pub fn gen_seed_phrase() -> Result<String, String> {
    let client = create_client_local()?;
    mnemonic_from_random(
        client,
        ParamsOfMnemonicFromRandom {
            dictionary: Some(MnemonicDictionary::English),
            word_count: Some(WORD_COUNT),
            ..Default::default()
        },
    )
    .map_err(|e| format!("{}", e))
    .map(|r| r.phrase)
}

pub fn generate_keypair_from_mnemonic(mnemonic: &str) -> Result<KeyPair, String> {
    let client = create_client_local()?;
    let hdk_master = hdkey_xprv_from_mnemonic(
        client.clone(),
        ParamsOfHDKeyXPrvFromMnemonic {
            dictionary: Some(MnemonicDictionary::English),
            word_count: Some(WORD_COUNT),
            phrase: mnemonic.to_string(),
            ..Default::default()
        },
    ).map_err(|e| format!("{}", e))?;

    let hdk_root = hdkey_derive_from_xprv_path(
        client.clone(),
        ParamsOfHDKeyDeriveFromXPrvPath {
            xprv: hdk_master.xprv,
            path: HD_PATH.to_string(),
            ..Default::default()
        },
    ).map_err(|e| format!("{}", e))?;

    let secret = hdkey_secret_from_xprv(
        client.clone(),
        ParamsOfHDKeySecretFromXPrv {
            xprv: hdk_root.xprv,
            ..Default::default()
        },
    ).map_err(|e| format!("{}", e))?;

    let mut keypair: KeyPair = nacl_sign_keypair_from_secret_key(
        client,
        ParamsOfNaclSignKeyPairFromSecret {
            secret: secret.secret,
            ..Default::default()
        },
    ).map_err(|e| format!("failed to get KeyPair from secret key: {}", e))?;

    // special case if secret contains public key too.
    let secret = hex::decode(&keypair.secret)
        .map_err(|e| format!("failed to decode the keypair: {}", e))?;
    if secret.len() > 32 {
        keypair.secret = hex::encode(&secret[..32]);
    }
    Ok(keypair)
}

pub fn generate_keypair_from_secret(secret: String) -> Result<KeyPair, String> {
    let client = create_client_local()?;
    let mut keypair: KeyPair = nacl_sign_keypair_from_secret_key(
        client,
        ParamsOfNaclSignKeyPairFromSecret {
            secret,
            ..Default::default()
        },
    ).map_err(|e| format!("failed to get KeyPair from secret key: {}", e))?;
    // special case if secret contains public key too.
    let secret = hex::decode(&keypair.secret)
        .map_err(|e| format!("failed to decode the keypair: {}", e))?;
    if secret.len() > 32 {
        keypair.secret = hex::encode(&secret[..32]);
    }
    Ok(keypair)
}

pub fn generate_mnemonic(keypath: Option<&str>, config: &Config) -> Result<(), String> {
    let mnemonic = gen_seed_phrase()?;
    if !config.is_json {
        println!("Succeeded.");
        println!(r#"Seed phrase: "{}""#, mnemonic);
    } else {
        println!("{{");
        println!("  \"phrase\": \"{}\"", mnemonic);
        println!("}}");
    }
    if let Some(path) = keypath {
        generate_keypair(Some(path), Some(&mnemonic), config)?;
        if !config.is_json {
            println!("Keypair saved to {}", path);
        }
    }
    Ok(())
}

pub fn extract_pubkey(mnemonic: &str, is_json: bool) -> Result<(), String> {
    let keypair = generate_keypair_from_mnemonic(mnemonic)?;
    if !is_json {
        println!("Succeeded.");
        println!("Public key: {}", keypair.public);
        println!();
        qr2term::print_qr(&keypair.public)
            .map_err(|e| format!("failed to print the QR code: {}", e))?;
        println!();
    } else {
        println!("{{");
        println!("  \"Public key\": \"{}\"", keypair.public);
        println!("}}");
    }
    Ok(())
}

pub fn generate_keypair(keys_path: Option<&str>, mnemonic: Option<&str>, config: &Config) -> Result<(), String> {
    let mnemonic = match mnemonic {
        Some(mnemonic) => mnemonic.to_owned(),
        None => {
            if !config.is_json {
                println!("Generating seed phrase.");
            }
            let phrase = gen_seed_phrase()?;
            if !config.is_json {
                println!(r#"Seed phrase: "{}""#, phrase);
            }
            phrase
        }
    };

    let keys = if mnemonic.contains(" ") {
        generate_keypair_from_mnemonic(&mnemonic)?
    } else {
        generate_keypair_from_secret(mnemonic)?
    };
    let keys_json = serde_json::to_string_pretty(&keys)
        .map_err(|e| format!("failed to serialize the keypair: {}", e))?;
    if let Some(keys_path) = keys_path {
        let folder_path = keys_path
            .trim_end_matches(|c| c != '/')
            .trim_end_matches(|c| c == '/');
        check_dir(folder_path)?;
        std::fs::write(keys_path, &keys_json)
            .map_err(|e| format!("failed to create file with keys: {}", e))?;
        if !config.is_json {
            println!("Keypair successfully saved to {}.", keys_path);
        }
    } else {
        if !config.is_json {
            print!("Keypair: ");
        }
        println!("{}", keys_json);
    }
    if !config.is_json {
        println!("Succeeded.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let mnemonic = "multiply extra monitor fog rocket defy attack right night jaguar hollow enlist";
        let keypair = generate_keypair_from_mnemonic(mnemonic).unwrap();
        assert_eq!(&keypair.public, "757221fe3d4992e44632e75e700aaf205d799cb7373ee929273daf26adf29e56");
        assert_eq!(&keypair.secret, "30e3bc5e67af2b0a72971bcc11256e83d052c6cb861a69a19a8af88922fadf3a");

        let mnemonic = "penalty nut enrich input palace flame safe session torch depth various hunt";
        let keypair = generate_keypair_from_mnemonic(mnemonic).unwrap();
        assert_eq!(&keypair.public, "8cf557aab2666867a1174e3147d89ddf28c2041a7322522276cd1cf1df47ae73");
        assert_eq!(&keypair.secret, "f63d3d11e0dc91f730f22d5397f269e01f1a5f984879c8581ac87f099bfd3b3a");
    }

    #[test]
    fn test_invalid_mnemonic() {
        let invalid_phrases = vec![
            "multiply extra monitor fog rocket defy attack right night jaguar hollow enlist ",
            "multiply  extra monitor fog rocket defy attack right night jaguar hollow enlist",
            "multipl extra monitor fog rocket defy attack right night jaguar hollow enlist",
            "s",
            "extra",
            "",
            " ",
            "123",
            "extra/1",
            "extra .1",
            "extra ,1",
            "0x0",
            "0:3333333333333333333333333333333333333333333333333333333333333333",
            "-alert()-",
            "multiply extra monitor fog rocket defy attack right night jaguar hollow enlist multiply extra monitor fog rocket defy attack right night jaguar hollow enlist multiply extra monitor fog rocket defy attack right night jaguar hollow enlist multiply extra monitor fog rocket defy attack right night jaguar hollow enlist multiply extra monitor fog rocket defy attack right night jaguar hollow enlist multiply extra monitor fog rocket defy attack right night jaguar hollow enlist"
        ];

        for phrase in invalid_phrases {
            assert!(generate_keypair_from_mnemonic(phrase).is_err());
        }
    }

}
