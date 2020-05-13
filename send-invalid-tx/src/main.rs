// Copyright Rivtower Technologies LLC.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use clap::Clap;
use git_version::git_version;
use log::info;
use tokio::runtime::Runtime;

const GIT_VERSION: &str = git_version!(
    args = ["--tags", "--always", "--dirty=-modified"],
    fallback = "unknown"
);
const GIT_HOMEPAGE: &str = "https://github.com/rink1969/cita_ng_tools";

/// network service
#[derive(Clap)]
#[clap(version = "0.1.0", author = "Rivtower Technologies.")]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    /// print information from git
    #[clap(name = "git")]
    GitInfo,
    /// run this service
    #[clap(name = "run")]
    Run(RunOpts),
}

/// A subcommand for run
#[derive(Clap)]
struct RunOpts {
    /// Sets grpc port of kms service.
    #[clap(short = "k", long = "kms_port", default_value = "50005")]
    kms_port: String,
    /// Sets grpc port of controller service.
    #[clap(short = "c", long = "controller_port", default_value = "50004")]
    controller_port: String,
}

fn main() {
    ::std::env::set_var("RUST_BACKTRACE", "full");

    let opts: Opts = Opts::parse();

    match opts.subcmd {
        SubCommand::GitInfo => {
            println!("git version: {}", GIT_VERSION);
            println!("homepage: {}", GIT_HOMEPAGE);
        }
        SubCommand::Run(opts) => {
            // init log4rs
            log4rs::init_file("tools-log4rs.yaml", Default::default()).unwrap();
            info!("grpc port of kms service: {}", opts.kms_port);
            info!("grpc port of controller service: {}", opts.controller_port);
            run(opts);
        }
    }
}

use cita_ng_proto::blockchain::{Transaction, UnverifiedTransaction, Witness};
use cita_ng_proto::controller::{
    raw_transaction::Tx, rpc_service_client::RpcServiceClient, Flag, RawTransaction,
};
use cita_ng_proto::kms::{
    kms_service_client::KmsServiceClient, GenerateKeyPairRequest, HashDataRequest,
    SignMessageRequest,
};
use prost::Message;
use tonic::Request;

fn build_tx(start_block_number: u64) -> Transaction {
    Transaction {
        version: 0,
        to: vec![1u8; 21],
        nonce: "test".to_owned(),
        quota: 300_000,
        valid_until_block: start_block_number + 80,
        data: vec![],
        value: vec![0u8; 32],
        chain_id: vec![0u8; 32],
    }
}

fn invalid_version_tx(start_block_number: u64) -> Transaction {
    Transaction {
        version: 1,
        to: vec![1u8; 21],
        nonce: "test".to_owned(),
        quota: 300_000,
        valid_until_block: start_block_number + 80,
        data: vec![],
        value: vec![0u8; 32],
        chain_id: vec![0u8; 32],
    }
}

fn invalid_nonce_tx(start_block_number: u64) -> Transaction {
    Transaction {
        version: 0,
        to: vec![1u8; 21],
        nonce: "1testtesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttesttest".to_owned(),
        quota: 300_000,
        valid_until_block: start_block_number + 80,
        data: vec![],
        value: vec![0u8; 32],
        chain_id: vec![0u8; 32],
    }
}

fn invalid_vub_tx1(start_block_number: u64) -> Transaction {
    Transaction {
        version: 0,
        to: vec![1u8; 21],
        nonce: "test".to_owned(),
        quota: 300_000,
        valid_until_block: start_block_number,
        data: vec![],
        value: vec![0u8; 32],
        chain_id: vec![0u8; 32],
    }
}

fn invalid_vub_tx2(start_block_number: u64) -> Transaction {
    Transaction {
        version: 0,
        to: vec![1u8; 21],
        nonce: "test".to_owned(),
        quota: 300_000,
        valid_until_block: start_block_number + 200,
        data: vec![],
        value: vec![0u8; 32],
        chain_id: vec![0u8; 32],
    }
}

fn invalid_value_tx(start_block_number: u64) -> Transaction {
    Transaction {
        version: 0,
        to: vec![1u8; 21],
        nonce: "test".to_owned(),
        quota: 300_000,
        valid_until_block: start_block_number + 80,
        data: vec![],
        value: vec![0u8; 31],
        chain_id: vec![0u8; 32],
    }
}

fn invalid_chain_id_tx(start_block_number: u64) -> Transaction {
    Transaction {
        version: 0,
        to: vec![1u8; 21],
        nonce: "test".to_owned(),
        quota: 300_000,
        valid_until_block: start_block_number + 80,
        data: vec![],
        value: vec![0u8; 32],
        chain_id: vec![0u8; 31],
    }
}

fn send_tx(
    address: Vec<u8>,
    key_id: u64,
    kms_port: String,
    controller_port: String,
    tx: Transaction,
) -> String {
    let mut rt = Runtime::new().unwrap();

    let kms_addr = format!("http://127.0.0.1:{}", kms_port);
    let controller_addr = format!("http://127.0.0.1:{}", controller_port);

    let mut kms_client = rt.block_on(KmsServiceClient::connect(kms_addr)).unwrap();
    let mut rpc_client = rt
        .block_on(RpcServiceClient::connect(controller_addr))
        .unwrap();

    // calc tx hash
    let mut tx_bytes = Vec::new();
    tx.encode(&mut tx_bytes).unwrap();
    let request = HashDataRequest {
        key_id,
        data: tx_bytes,
    };
    let ret = rt.block_on(kms_client.hash_data(request)).unwrap();
    let tx_hash = ret.into_inner().hash;

    // sign tx hash
    let request = Request::new(SignMessageRequest {
        key_id,
        msg: tx_hash.clone(),
    });
    let ret = rt.block_on(kms_client.sign_message(request)).unwrap();
    let signature = ret.into_inner().signature;

    let witness = Witness {
        signature,
        sender: address,
    };

    let unverified_tx = UnverifiedTransaction {
        transaction: Some(tx),
        transaction_hash: tx_hash,
        witness: Some(witness),
    };

    let raw_tx = RawTransaction {
        tx: Some(Tx::NormalTx(unverified_tx)),
    };

    let ret = rt.block_on(rpc_client.send_raw_transaction(raw_tx));
    match ret {
        Ok(response) => {
            info!("tx hash {:?}", response.into_inner().hash);
            "".to_owned()
        }
        Err(status) => {
            info!("err {}", status.message());
            status.message().to_owned()
        }
    }
}

fn run(opts: RunOpts) {
    let kms_port = opts.kms_port;
    let controller_port = opts.controller_port;

    let mut rt = Runtime::new().unwrap();

    let kms_addr = format!("http://127.0.0.1:{}", kms_port.clone());
    let controller_addr = format!("http://127.0.0.1:{}", controller_port.clone());

    let mut kms_client = rt.block_on(KmsServiceClient::connect(kms_addr)).unwrap();
    let mut rpc_client = rt
        .block_on(RpcServiceClient::connect(controller_addr))
        .unwrap();

    // generate_key_pair for sign tx
    let request = Request::new(GenerateKeyPairRequest {
        crypt_type: 1,
        description: "test".to_owned(),
    });
    let ret = rt.block_on(kms_client.generate_key_pair(request)).unwrap();
    let response = ret.into_inner();
    let key_id = response.key_id;
    let address = response.address;

    info!("key id is {}", key_id);

    // get block number
    let request = Request::new(Flag { flag: false });
    let ret = rt.block_on(rpc_client.get_block_number(request)).unwrap();
    let start_block_number = ret.into_inner().block_number;
    info!("block_number is {} before start", start_block_number);

    // ok
    assert_eq!(
        send_tx(
            address.clone(),
            key_id,
            kms_port.clone(),
            controller_port.clone(),
            build_tx(start_block_number),
        ),
        "".to_owned()
    );

    // dup
    assert_eq!(
        send_tx(
            address.clone(),
            key_id,
            kms_port.clone(),
            controller_port.clone(),
            build_tx(start_block_number),
        ),
        "dup".to_owned()
    );

    assert_eq!(
        send_tx(
            address.clone(),
            key_id,
            kms_port.clone(),
            controller_port.clone(),
            invalid_version_tx(start_block_number),
        ),
        "Invalid version".to_owned()
    );

    assert_eq!(
        send_tx(
            address.clone(),
            key_id,
            kms_port.clone(),
            controller_port.clone(),
            invalid_nonce_tx(start_block_number),
        ),
        "Invalid nonce".to_owned()
    );

    assert_eq!(
        send_tx(
            address.clone(),
            key_id,
            kms_port.clone(),
            controller_port.clone(),
            invalid_vub_tx1(start_block_number),
        ),
        "Invalid valid_until_block".to_owned()
    );

    assert_eq!(
        send_tx(
            address.clone(),
            key_id,
            kms_port.clone(),
            controller_port.clone(),
            invalid_vub_tx2(start_block_number),
        ),
        "Invalid valid_until_block".to_owned()
    );

    assert_eq!(
        send_tx(
            address.clone(),
            key_id,
            kms_port.clone(),
            controller_port.clone(),
            invalid_value_tx(start_block_number),
        ),
        "Invalid value".to_owned()
    );

    assert_eq!(
        send_tx(
            address.clone(),
            key_id,
            kms_port.clone(),
            controller_port.clone(),
            invalid_chain_id_tx(start_block_number),
        ),
        "Invalid chain_id".to_owned()
    );
}
