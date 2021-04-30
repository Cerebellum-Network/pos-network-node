// Offchain worker for DDC metrics.
//
// Inspired from https://github.com/paritytech/substrate/tree/master/frame/example-offchain-worker

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod tests;

use frame_support::traits::Get;
use frame_system::{
	offchain::{
		AppCrypto, SendSignedTransaction,
		SigningTypes, Signer, CreateSignedTransaction
	}
};
use frame_support::{
	debug, decl_module, decl_storage, decl_event,
};
use sp_core::crypto::KeyTypeId;
use sp_runtime::{
	offchain::{
		http,
		Duration,
		storage::StorageValueRef
	},
	traits::StaticLookup,
	traits::Zero,
};
use codec::{Encode, Decode};
use sp_std::{vec::Vec, str::from_utf8};
use pallet_contracts;
// use lite_json::json::JsonValue;
use alt_serde::{Deserialize, Deserializer};
use hex_literal::hex;

#[macro_use]
extern crate alloc;

// The address of the metrics contract, in SS58 and in bytes formats.
// To change the address, see tests/mod.rs decode_contract_address().
pub const METRICS_CONTRACT_ADDR: &str = "5Ch5xtvoFF3Muu91WkHCY4mhTDTCyYS2TmBL1zKiBXrYbiZv";
pub const METRICS_CONTRACT_ID: [u8; 32] = [27, 191, 65, 45, 0, 189, 12, 234, 31, 196, 9, 143, 196, 27, 157, 170, 92, 57, 127, 122, 70, 152, 19, 223, 235, 21, 170, 26, 249, 130, 98, 114];
pub const BLOCK_INTERVAL: u32 = 10;

pub const REPORT_METRICS_SELECTOR: [u8; 4] = hex!("35320bbe");

// use serde_json::{Value};

// Specifying serde path as `alt_serde`
// ref: https://serde.rs/container-attrs.html#crate
#[serde(crate = "alt_serde")]
#[derive(Deserialize, Encode, Decode, Default)]
#[derive(Debug)]
#[allow(non_snake_case)]
struct NodeInfo {
	#[serde(deserialize_with = "de_string_to_bytes")]
	id: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	httpAddr: Vec<u8>,
	totalPartitions: u32,
	reservedPartitions: u32,
	availablePartitions: u32,
	#[serde(deserialize_with = "de_string_to_bytes")]
	status: Vec<u8>,
	deleted: bool,
}

#[serde(crate = "alt_serde")]
#[derive(Deserialize, Encode, Decode, Default)]
#[derive(Debug)]
#[allow(non_snake_case)]
struct PartitionInfo {
	#[serde(deserialize_with = "de_string_to_bytes")]
	id: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	appPubKey: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	nodeId: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	status: Vec<u8>,
	isMaster: bool,
	ringToken: u32,
	backup: bool,
	deleted: bool,
	replicaIndex: u32,
}

#[serde(crate = "alt_serde")]
#[derive(Deserialize, Encode, Decode, Default)]
#[derive(Debug)]
#[allow(non_snake_case)]
struct MetricInfo {
	#[serde(deserialize_with = "de_string_to_bytes")]
	appPubKey: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	partitionId: Vec<u8>,
	bytes: u32,
	requests: u32,
}

pub fn de_string_to_bytes<'de, D>(de: D) -> Result<Vec<u8>, D::Error>
	where
		D: Deserializer<'de>,
{
	let s: &str = Deserialize::deserialize(de)?;
	Ok(s.as_bytes().to_vec())
}


/// Defines application identifier for crypto keys of this module.
///
/// Every module that deals with signatures needs to declare its unique identifier for
/// its crypto keys.
/// When offchain worker is signing transactions it's going to request keys of type
/// `KeyTypeId` from the keystore and use the ones it finds to sign the transaction.
/// The keys can be inserted manually via RPC (see `author_insertKey`).
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"ddc1");

pub const HTTP_NODES: &str = "/api/rest/nodes";
pub const HTTP_PARTITIONS: &str = "/api/rest/partitions?isMaster=true&active=true";
pub const HTTP_METRICS: &str = "/api/rest/metrics?appPubKey=";
pub const METRICS_PARAM_PARTITIONID: &str = "&partitionId="; // partition.id
pub const METRICS_PARAM_FROM: &str = "&from="; // lastTimestamp
pub const METRICS_PARAM_TO: &str = "&to="; // now() - 2 minutes
pub const FETCH_TIMEOUT_PERIOD: u64 = 3000; // in milli-seconds

/// Based on the above `KeyTypeId` we need to generate a pallet-specific crypto type wrappers.
/// We can use from supported crypto kinds (`sr25519`, `ed25519` and `ecdsa`) and augment
/// the types with this pallet-specific identifier.
pub mod crypto {
	use super::KEY_TYPE;
	use sp_runtime::{
		app_crypto::{app_crypto, sr25519},
		traits::Verify,
	};
	use frame_system::offchain::AppCrypto;
	use sp_core::sr25519::Signature as Sr25519Signature;
	app_crypto!(sr25519, KEY_TYPE);

	use sp_runtime::{
		MultiSignature,
		MultiSigner,
	};

	pub struct TestAuthId;
	impl AppCrypto<<Sr25519Signature as Verify>::Signer, Sr25519Signature> for TestAuthId {
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}

	impl AppCrypto<MultiSigner, MultiSignature> for TestAuthId {
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}
}



decl_module! {
	/// A public part of the pallet.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		//fn deposit_event() = default;

		/// Offchain Worker entry point.
		///
		/// By implementing `fn offchain_worker` within `decl_module!` you declare a new offchain
		/// worker.
		/// This function will be called when the node is fully synced and a new best block is
		/// succesfuly imported.
		/// Note that it's not guaranteed for offchain workers to run on EVERY block, there might
		/// be cases where some blocks are skipped, or for some the worker runs twice (re-orgs),
		/// so the code should be able to handle that.
		/// You can use `Local Storage` API to coordinate runs of the worker.
		/// You can use `debug::native` namespace to not log in wasm mode.
		fn offchain_worker(block_number: T::BlockNumber) {
			debug::info!("[OCW] Hello World from offchain workers!");

			let check_result = Self::check_if_should_proceed(block_number);

			if check_result == false {
				debug::info!("[OCW] Too early to execute OCW. Skipping");
				return; // Skip the execution for this block
			}

			let res = Self::fetch_ddc_data_and_send_to_sc(block_number);

			// TODO: abort on error.
			if let Err(e) = res {
				debug::error!("Some error occurred: {:?}", e);
			}

			debug::info!("[OCW] About to send mock transaction");
			let sc_res = Self::send_to_sc_mock();

			if let Err(e) = sc_res {
				debug::error!("Some error occurred: {:?}", e);
			}

            debug::info!("[OCW] Finishing!");
		}
	}
}

decl_storage! {
	trait Store for Module<T: Trait> as ExampleOffchainWorker {
	}
}

impl<T: Trait> Module<T> {
	fn check_if_should_proceed(_block_number: T::BlockNumber) -> bool {
		let s_next_at = StorageValueRef::persistent(b"example-offchain-worker::next-at"); // TODO: Rename after OCW renamed

		match s_next_at.mutate(
			|current_next_at| {
				let current_next_at = current_next_at.unwrap_or(Some(T::BlockNumber::default()));

				if let Some(current_next_at) = current_next_at {
					if current_next_at > _block_number {
						debug::info!("[OCW] Too early to execute. Current: {:?}, next execution at: {:?}", _block_number, current_next_at);
						Err("Skipping")
					} else {
						// set new value
						Ok(T::BlockInterval::get() + _block_number)
					}
				} else {
					debug::error!("[OCW] Something went wrong in `check_if_should_proceed`");
					Err("Unexpected error")
				}
			}
		) {
			Ok(_val) => true,
			Err(_e) => {
				false
			}
		}
	}


	fn send_to_sc_mock() -> Result<(), &'static str> {

		debug::info!("[OCW] Getting signer");
		let signer = Signer::<T::CST, T::AuthorityId>::any_account();
		debug::info!("[OCW] Checking signer");
		if !signer.can_sign() {
			return Err(
				"No local accounts available. Consider adding one via `author_insertKey` RPC."
			)?
		}

		debug::info!("[OCW] Continue, signer exists!");

		// Using `send_signed_transaction` associated type we create and submit a transaction
		// representing the call, we've just created.
		// Submit signed will return a vector of results for all accounts that were found in the
		// local keystore with expected `KEY_TYPE`.
		let results = signer.send_signed_transaction(
			|_account| {
				let mut call_data = REPORT_METRICS_SELECTOR.to_vec();
				call_data.extend_from_slice(&[
					1,
					2,
					3,
					4,
				]);

				pallet_contracts::Call::call(
					T::ContractId::get(),
					0u32.into(),
					10_000_000_000,
					call_data
				)
			}
		);

		for (_acc, res) in &results {
			match res {
				Ok(()) => debug::info!("Submitted TX to SC!"),
				Err(e) => debug::error!("Some error occured: {:?}", e),
			}
		}

		Ok(())
	}


	fn fetch_ddc_data_and_send_to_sc(_block_number: T::BlockNumber) -> Result<(), http::Error> {
		let topology = Self::fetch_ddc_network_topology().map_err(|_| "Failed to fetch topology");

		debug::info!("Network topology: {:?}", topology);

		Ok(())
	}

//	fn get_nodes_data(topology: Vec<u32>) -> Result<u32, http::Error> {
//		debug::warn("Topology: {}", topology)
//	}

	/// This function uses the `offchain::http` API to query data
	/// For get method, input url and returns the JSON response as vector of bytes.
	fn http_get_request(http_url: &str) -> Result<Vec<u8>, http::Error> {
		debug::info!("Sending request to: {:?}", http_url);

		// Initiate an external HTTP GET request. This is using high-level wrappers from `sp_runtime`.
		let request = http::Request::get(http_url);

		let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(FETCH_TIMEOUT_PERIOD));

		let pending = request
			.deadline(deadline)
			.send()
			.map_err(|_| http::Error::IoError)?;

		let response = pending.try_wait(deadline)
			.map_err(|_| http::Error::DeadlineReached)??;

		if response.code != 200 {
			debug::warn!("http_get_request unexpected status code: {}", response.code);
			return Err(http::Error::Unknown);
		}

		// Next we fully read the response body and collect it to a vector of bytes.
		Ok(response.body().collect::<Vec<u8>>())
	}

	/// This function uses the `offchain::http` API to query the remote github information,
	///   and returns the JSON response as vector of bytes.
	fn fetch_from_node(one_node: &NodeInfo) -> Result<Vec<u8>, http::Error> {
		let node_url = sp_std::str::from_utf8(&one_node.httpAddr).unwrap();

		let response_data = Self::http_get_request(node_url).map_err(|e| {
			debug::error!("fetch_from_node error: {:?}", e);
			http::Error::Unknown
		})?;

		// Next we fully read the response body and collect it to a vector of bytes.
		Ok(response_data)
	}

	fn fetch_ddc_network_topology() -> Result<(), http::Error> {
		let mut url = T::DdcUrl::get();
		url.extend_from_slice(HTTP_NODES.as_bytes());
		let url = from_utf8(&url).unwrap();

		let fetch_node_list_bytes = Self::http_get_request(url).map_err(|e| {
			debug::error!("fetch_from_node error: {:?}", e);
			http::Error::Unknown
		})?;

		let fetch_node_list_str = sp_std::str::from_utf8(&fetch_node_list_bytes).map_err(|_| {
			debug::warn!("fetch_node_list_str: No UTF8 body");
			http::Error::Unknown
		})?;

		// debug::info!("fetch_node_list_str: {}", fetch_node_list_str);
		// Parse the string of data into serde_json::Value.
		let node_info: Vec<NodeInfo> = serde_json::from_str(&fetch_node_list_str).map_err(|_| {
			debug::warn!("Parse body to Vec<NodeInfo> error");
			http::Error::Unknown
		})?;
		debug::info!("node_info length: {:?}", node_info.len());

		let mut url_partitions = T::DdcUrl::get();
		url_partitions.extend_from_slice(HTTP_PARTITIONS.as_bytes());
		let url_partitions = from_utf8(&url_partitions).unwrap();

		let fetch_partition_list_bytes = Self::http_get_request(url_partitions).map_err(|e| {
			debug::error!("fetch_partition_list_bytes error: {:?}", e);
			http::Error::Unknown
		})?;

		let fetch_partition_list_str = sp_std::str::from_utf8(&fetch_partition_list_bytes).map_err(|_| {
			debug::warn!("fetch_partition_list_str: No UTF8 body");
			http::Error::Unknown
		})?;
		// debug::info!("fetch_partition_list_str: {}", fetch_partition_list_str);

		// Parse the string of data into serde_json::Value.
		let partition_info: Vec<PartitionInfo> = serde_json::from_str(&fetch_partition_list_str).map_err(|_| {
			debug::warn!("Parse body to Vec<PartitionInfo> error");
			http::Error::Unknown
		})?;
		debug::info!("Partition_info length: {:#?}", partition_info.len());

		let mut node_info_iter = node_info.iter(); // id is Vec<u8>
		// debug::info!("node_info_iter: {:?}", node_info_iter);

		for one_partition in partition_info.iter() {
			let mut id_from_partition = sp_std::str::from_utf8(&one_partition.nodeId).unwrap();

			let mut equal_index = node_info_iter.position(|node| id_from_partition.eq(sp_std::str::from_utf8(&node.id).unwrap()));
			// debug::info!("id_from_partition: {}", id_from_partition);
			debug::info!("equal_index: {:?}", &equal_index);

			if equal_index.is_none() {
				debug::info!("Can not found in nodeId {:?} in the topology", sp_std::str::from_utf8(&one_partition.nodeId).unwrap());
				continue;
			}

			let metrics_url = sp_std::str::from_utf8(&node_info[equal_index.unwrap()].httpAddr).map_err(|_| {
				debug::warn!("httpAddr");
				http::Error::Unknown
			})?;

			let app_pubkey_str = sp_std::str::from_utf8(&one_partition.appPubKey).map_err(|_| {
				debug::warn!("appPubKey error");
				http::Error::Unknown
			})?;
			let metrics_url_with_key = format!("{}{}{}", metrics_url, HTTP_METRICS, app_pubkey_str);
			// debug::info!("metrics_url_with_key: {}", metrics_url_with_key);

			let partition_id_str = sp_std::str::from_utf8(&one_partition.id).map_err(|_| {
				debug::warn!("partition id error");
				http::Error::Unknown
			})?;

			let metrics_url_with_partition = format!("{}{}{}", metrics_url_with_key, METRICS_PARAM_PARTITIONID, partition_id_str);
			// debug::info!("metrics_url_with_partition: {}", metrics_url_with_partition);

			let last_time_stamp: u64 = 1619690465923;
			let metrics_url_with_last_time = format!("{}{}{}", metrics_url_with_partition, METRICS_PARAM_FROM, last_time_stamp);
			// debug::info!("metrics_url_with_last_time: {}", metrics_url_with_last_time);

			let to_time = sp_io::offchain::timestamp().sub(Duration::from_millis(120_000));
			let metrics_url_final = format!("{}{}{}", metrics_url_with_last_time, METRICS_PARAM_TO, to_time.unix_millis());
			// debug::info!("to_time: {:?}", to_time);

			let fetch_metric_bytes = Self::http_get_request(&metrics_url_final).map_err(|e| {
				debug::error!("fetch_metric_bytes error: {:?}", e);
				http::Error::Unknown
			})?;

			let fetch_metric_str = sp_std::str::from_utf8(&fetch_metric_bytes).map_err(|_| {
				debug::warn!("fetch_metric_str: No UTF8 body");
				http::Error::Unknown
			})?;

			let fetch_metric: Vec<MetricInfo> = serde_json::from_str(&fetch_metric_str).map_err(|_| {
				debug::warn!("Parse body to Vec<NodeInfo> error");
				http::Error::Unknown
			})?;
			debug::info!("fetch_metric length: {:?}", fetch_metric.len());

			for one_metric in fetch_metric.iter() {
				debug::info!("Metric item. App: {:?}, bytes: {:?}, requests: {:?}", sp_std::str::from_utf8(&one_metric.appPubKey).unwrap(), one_metric.bytes, one_metric.requests);
			}
		}
	
		Ok(())
	}

	// fn parse_topology(topology_str: &str) -> Result<(), http::Error> {
	// 	let val = lite_json::parse_json(topology_str).ok();

	// 	Ok(())

}


// TODO: remove, or write meaningful events.
decl_event!(
	/// Events generated by the module.
	pub enum Event<T> where AccountId = <T as frame_system::Trait>::AccountId {
		NewDdcMetric(AccountId, Vec<u8>),
	}
);


pub trait Trait: frame_system::Trait {
	type ContractId: Get<<<Self::CT as frame_system::Trait>::Lookup as StaticLookup>::Source>;
	type DdcUrl: Get<Vec<u8>>;

	type CT: pallet_contracts::Trait;
	type CST: CreateSignedTransaction<pallet_contracts::Call<Self::CT>>;

	/// The identifier type for an offchain worker.
	type AuthorityId: AppCrypto<<Self::CST as SigningTypes>::Public, <Self::CST as SigningTypes>::Signature>;

	// TODO: remove, or use Event and Call.
	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
	/// The overarching dispatch call type.
	type Call: From<Call<Self>>;

	type BlockInterval: Get<Self::BlockNumber>;
}
