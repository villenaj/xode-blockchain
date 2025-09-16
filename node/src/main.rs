//! Substrate Parachain Node Template CLI

#![warn(missing_docs)]
use sp_core::crypto::{Ss58AddressFormat, set_default_ss58_version};

mod chain_spec;
mod cli;
mod command;
mod rpc;
mod service;

fn main() -> sc_cli::Result<()> {
	set_default_ss58_version(Ss58AddressFormat::custom(280));
	command::run()
}
