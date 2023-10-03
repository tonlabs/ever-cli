# Changelog

All notable changes to this project will be documented in this file.

## 0.35.8

### Fixes
- Fixed clippy warnings

## 0.35.7

### Bug fixes
- Fixed double log initialization bug for runx subcommand

## 0.35.6

### New
- Fixed double log initialization bug

## 0.35.5

### New
- Migrated to ever-sdk 1.43.3

## 0.35.4

### New 
- Added `test` command and subcommands: `config`, `deploy`, `sign`, `ticktock`
- Added ability not to receive debug output for `debug` command using `nul` for output file name
- Added ability to set `initial_balance` for account deployment

## 0.34.1

### New 
- Fixed update_config command bug for solidity contracts

## 0.34.0

### New 
- Flag `--v2` in `multisig` and `depool` subcommands to support multisig v2.

## 0.33.0

### New
- Migrated to ever-sdk 0.41.1

## Version: 0.30.1

### New
- Added the `sign` command. It makes ED25519 signature for data encoded in base64 or hex using common `--keys` option;

## Version: 0.29.1

### New
- Added [sold](https://github.com/tonlabs/TON-Solidity-Compiler/tree/master/sold) functionality as feature;
- Improved behavior of the `decode msg` command. Now it doesn't require `--base64` flag to decode base64 input. It can
  also decode message by id in the blockchain and decode files with messages not in binary but with text in base64;
- Changed `debug transaction` and `debug account` commands flag `--empty_config` to `--default_config` which uses 
  current network config or default one if it is unavailable;
- Removed option `--saved_config` from call and run commands.

## Version: 0.28.12

### New
- Added ability to specify link to the abi file of json data instead of path.

## Version: 0.28.3

### Breaking changes:
 - `debug` commands `call`, `run` and `deploy` now take function parameters in alternative syntax. 

## Version: 0.28.1

### New
 - Updated version of SDK;
 - Added global tonos-cli config which is used to create default configs;
 - Added config parameters for Evercloud authentication;
 - Added new input format for `tonos-cli decode message` command.

## Version: 0.27.33

### New
- Added ability to call `account` command with address from config

### Bug fixes
- Fixed work with old config file


## Version: 0.27.31

### New
 - Clear alternative syntax parameters
 - Alias and abi methods completion


## Version: 0.27.30

### New
- Added alias functionality
- Added completion script to complete bash commands with aliases and abi methods.


## Version: 0.27.26

### New
- Added `--now <value>` option for `debug message` command.

## Version: 0.27.20

### New
- Enlarged decode fields for `decode body` command
- Added sequence diagram rendering command

## Version: 0.27.19

### Bug fixes
- Removed custom header from call command

## Version: 0.27.6

### Bug fixes
- Fixed `debug run` gas limits


## Version: 0.27.1

### Breaking changes:
 - Commands `convert` and `callex` were removed.


## Version: 0.26.45

### New
 - `tokio` library updated to `1.*` version
 
## Version: 0.26.44

### New


## Version: 0.26.35

### New
 - Add config param 42
 - Update libraries


## Version: 0.26.34

### New
 - Update libraries


## Version: 0.26.30

### New


## Version: 0.26.28

### New
 - Added network test and improved giver for parallel debot tests
 - Added Ubuntu 22 hint
 - Fixed tests to work in parallel


## Version: 0.26.26

### New
 - Fixed tests to work in parallel


## Version: 0.26.24

### New
  - Libraries update

## Version: 0.26.8

### New
 - Update endpoints
 - Added --boc flag for account command


## Version: 0.26.7

### New


## Version: 0.26.4

### New


## Version: 0.26.1

### New
 - Breaking change for getkeypair command: arguments are now specified with flags and can be omitted.


## Version: 0.25.23

### New

## Version: 0.25.15


### New
 - Add support copyleft

 
## Version: 0.25.11

### New


## Version: 0.25.7

### New
 - Unify genaddr abi param with other cmds
 - Add &#x60;account-wait&#x60; subcommand
 - Fixed client creation for local run
 - Fixed a bug with run parameters
 - Fixed runget
 - Refactored and improved debug on fail
 - inverted min_trace flag


## Version: 0.25.6

### New
 - Add &#x60;account-wait&#x60; subcommand
 - Fixed client creation for local run
 - Fixed a bug with run parameters
 - Fixed runget
 - Refactored and improved debug on fail
 - inverted min_trace flag


## Version: 0.25.3

### New
 - Refactored and improved debug on fail
 - inverted min_trace flag


## Version: 0.25.2

### New
 - Refactored and improved debug on fail
 - inverted min_trace flag


## Version: 0.24.59

### New
 - Block replaying
 - inverted min_trace flag


## Version: 0.24.56

### New


## Version: 0.24.51

### New


## Version: 0.24.48

### New


## Version: 0.24.46

### New
