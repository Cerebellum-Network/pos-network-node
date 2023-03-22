#![cfg_attr(not(feature = "std"), no_std)]

pub use alloc::{format, string::String};
pub use alt_serde::{de::DeserializeOwned, Deserialize, Serialize};
pub use codec::{Encode, Decode, MaxEncodedLen, HasCompact};
pub use core::fmt::Debug;
pub use frame_support::{
	decl_event, decl_module, decl_storage,
	log::{error, info, warn},
	pallet_prelude::*, 
	traits::{Randomness, Currency}, 
	weights::Weight, 
	dispatch::DispatchResult,
	RuntimeDebug,
	BoundedVec,
	parameter_types,
};
pub use frame_system::{ensure_signed, pallet_prelude::*, offchain::{CreateSignedTransaction, Signer, SigningTypes, AppCrypto, SendSignedTransaction}};
pub use pallet::*;
pub use pallet_ddc_staking::{self as ddc_staking};
pub use pallet_staking::{self as staking};
pub use pallet_session as session;
pub use scale_info::TypeInfo;
pub use sp_core::crypto::{KeyTypeId, UncheckedFrom};
pub use sp_runtime::offchain::{http, Duration};
pub use sp_std::prelude::*;
pub use sp_io::offchain::timestamp;
extern crate alloc;

parameter_types! {
	pub DdcValidatorsQuorumSize: u32 = 3;
}

type BalanceOf<T> = <<T as pallet_contracts::Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::Balance;

type ResultStr<T> = Result<T, &'static str>;


pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"dacv");

pub const HTTP_TIMEOUT_MS: u64 = 30_000;

const TIME_START_MS: u64 = 1_672_531_200_000;
const ERA_DURATION_MS: u64 = 120_000;
const ERA_IN_BLOCKS: u8 = 20;

const DATA_PROVIDER_URL: &str = "http://localhost:7379/";

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum ValidationMethodKind {
	ProofOfDelivery,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Decision<AccountId> {
	pub decision: Option<bool>,
	pub method: ValidationMethodKind,
	pub validator: AccountId,
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo, Default)]
pub struct ValidationResult<AccountId> {
    era: String,
    signer: AccountId,
    val_res: bool,
    cdn_node_pub_key: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(crate = "alt_serde")]
#[serde(rename_all = "camelCase")]
pub struct RedisFtAggregate {
    #[serde(rename = "FT.AGGREGATE")]
    pub ft_aggregate: (u32, Vec<String>, Vec<String>),
}

#[derive(Clone)]
struct BytesSent {
    node_public_key: String,
    era: String,
    sum: u32,
}

impl BytesSent {
    pub fn new(aggregate: RedisFtAggregate) -> BytesSent {
        let (_, values, values2) = aggregate.ft_aggregate;

        BytesSent {
            node_public_key: values[1].clone(),
            era: values[3].clone(),
            sum: values[5].parse::<u32>().expect("bytesSentSum must be convertable to u32"),
        }
    }
}

#[derive(Clone)]
struct BytesReceived {
    node_public_key: String,
    era: String,
    sum: u32,
}

impl BytesReceived {
    pub fn new(aggregate: RedisFtAggregate) -> BytesReceived {
        let (_, values, values2) = aggregate.ft_aggregate;

        BytesReceived {
            node_public_key: values[1].clone(),
            era: values[3].clone(),
            sum: values[5].parse::<u32>().expect("bytesReceivedSum must be convertable to u32"),
        }
    }
}

pub mod crypto {
	use super::KEY_TYPE;
	use frame_system::offchain::AppCrypto;
	use sp_core::sr25519::Signature as Sr25519Signature;
	use sp_runtime::{
			app_crypto::{app_crypto, sr25519},
			traits::Verify,
	};
	app_crypto!(sr25519, KEY_TYPE);

	use sp_runtime::{MultiSignature, MultiSigner};

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

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: 
		frame_system::Config
		+ pallet_contracts::Config
    + pallet_session::Config<ValidatorId = <Self as frame_system::Config>::AccountId>
		+ pallet_staking::Config
		+ ddc_staking::Config
		+ CreateSignedTransaction<Call<Self>>
			where
				<Self as frame_system::Config>::AccountId: AsRef<[u8]> + UncheckedFrom<Self::Hash>,
				<BalanceOf<Self> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
	{
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type Randomness: Randomness<Self::Hash, Self::BlockNumber>;
		type Call: From<Call<Self>>;
    type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
	}

	#[pallet::storage]
	#[pallet::getter(fn tasks)]
	pub type Tasks<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		BoundedVec<Decision<T::AccountId>, DdcValidatorsQuorumSize>,
	>;

	#[pallet::storage]
	#[pallet::getter(fn last_managed_era)]
	pub type LastManagedEra<T: Config> = StorageValue<_, u64>;

	#[pallet::storage]
	#[pallet::getter(fn validation_results)]
	pub(super) type ValidationResults<T: Config> = StorageValue<_, Vec<ValidationResult::<T::AccountId>>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> 
	where
	<T as frame_system::Config>::AccountId: AsRef<[u8]> + UncheckedFrom<T::Hash>,
	<BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
	{}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> 
	where
        <T as frame_system::Config>::AccountId: AsRef<[u8]> + UncheckedFrom<T::Hash>,
        <BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
	{
		fn on_initialize(block_number: T::BlockNumber) -> Weight {
			match (Self::get_current_era(), <LastManagedEra<T>>::get()) {
				(global_era_counter, Some(last_managed_era)) => {
					if last_managed_era >= global_era_counter {
						return 0
					}
					<LastManagedEra<T>>::put(global_era_counter);
				},
				(global_era_counter, None) => {
					<LastManagedEra<T>>::put(global_era_counter);
				},
			};

			let validators: Vec<T::AccountId> = <staking::Validators<T>>::iter_keys().collect();
			let validators_count = validators.len() as u32;
			let edges: Vec<T::AccountId> = <ddc_staking::pallet::Edges<T>>::iter_keys().collect();
			log::info!(
				"Block number: {:?}, global era: {:?}, last era: {:?}, validators_count: {:?}, validators: {:?}, edges: {:?}",
				block_number,
				Self::get_current_era(),
				<LastManagedEra<T>>::get(),
				validators_count,
				validators,
				edges,
			);

			// A naive approach assigns random validators for each edge.
			for edge in edges {
				let mut decisions: BoundedVec<Decision<T::AccountId>, DdcValidatorsQuorumSize> =
					Default::default();
				while !decisions.is_full() {
					let validator_idx = Self::choose(validators_count).unwrap_or(0) as usize;
					let validator: T::AccountId = validators[validator_idx].clone();
					let assignment = Decision {
						validator,
						method: ValidationMethodKind::ProofOfDelivery,
						decision: None,
					};
					decisions.try_push(assignment).unwrap();
				}
				Tasks::<T>::insert(edge, decisions);
			}
			0
		}

		fn offchain_worker(block_number: T::BlockNumber) {
			let res = Self::offchain_worker_main(block_number);

			match res {
				Ok(()) => info!("[DAC Validator] DAC Validator is suspended."),
				Err(err) => error!("[DAC Validator] Error in Offchain Worker: {}", err),
			};
		}
	}

	#[pallet::call]
    impl<T: Config> Pallet<T>
    where
        <T as frame_system::Config>::AccountId: AsRef<[u8]> + UncheckedFrom<T::Hash>,
        <BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
	{
		#[pallet::weight(10000)]
		pub fn save_validated_data(origin: OriginFor<T>, val_res: bool, cdn_node_pub_key: String, era: String) -> DispatchResult {
			let signer: T::AccountId = ensure_signed(origin)?;

			info!("[DAC Validator] author: {:?}", signer);
			let mut v_results = ValidationResults::<T>::get();

			let cur_validation = ValidationResult::<T::AccountId> {
					era,
					val_res,
					cdn_node_pub_key,
					signer,
			};

			v_results.push(cur_validation);

			ValidationResults::<T>::set(v_results);

			Ok(())
		}

		#[pallet::weight(10000)]
		pub fn proof_of_delivery(origin: OriginFor<T>, era: u64) -> DispatchResult {
			let signer: T::AccountId = ensure_signed(origin)?;

			let cdn_nodes_to_validate = Self::fetch_tasks(&signer);
			for cdn_node_id in cdn_nodes_to_validate {
				let (bytes_sent, bytes_received) = Self::fetch_data(era, &cdn_node_id);
				let val_res = Self::validate(bytes_sent.clone(), bytes_received.clone());

				let decisions_for_cdn = <Tasks<T>>::get(cdn_node_id);
				for decision in decisions_for_cdn.unwrap().iter_mut() {
					if decision.validator == signer {
						decision.decision = Some(val_res);
						decision.method = ValidationMethodKind::ProofOfDelivery;
					}
				}
			}

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> 
	where
        <T as frame_system::Config>::AccountId: AsRef<[u8]> + UncheckedFrom<T::Hash>,
        <BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
	{
		fn offchain_worker_main(block_number: T::BlockNumber) -> ResultStr<()> {
			info!("[DAC Validator] Validation data stored onchain: {:?}", ValidationResults::<T>::get());

			if block_number % ERA_IN_BLOCKS.into() != 0u32.into() {
					return Ok(())
			}

			let signer = match Self::get_signer() {
					Err(e) => {
							warn!("{:?}", e);
							return Ok(());
					}
					Ok(signer) => signer,
			};

			info!("[DAC Validator] ValidationResults: {:?}", ValidationResults::<T>::get());

			// Read data from DataModel and do dumb validation
			let current_era = Self::get_current_era() - 1u64;

	
			let tx_res = signer.send_signed_transaction(|_acct| {
					info!("[DAC Validator] Trigger proof of delivery");

					// This is the on-chain function
					Call::proof_of_delivery { era: current_era }
			});

			match &tx_res {
					None | Some((_, Err(()))) => {
							return Err("Error while submitting proof of delivery TX")
					}
					Some((_, Ok(()))) => {}
			}

			Ok(())
		}

		fn get_signer() -> ResultStr<Signer<T, T::AuthorityId>> {
			let signer = Signer::<_, _>::any_account();
			if !signer.can_sign() {
					return Err("[DAC Validator] No local accounts available. Consider adding one via `author_insertKey` RPC.");
			}

			Ok(signer)
		}

		// Get the current era; Shall we start era count from 0 or from 1?
		fn get_current_era() -> u64 {
			(timestamp().unix_millis() - TIME_START_MS) / ERA_DURATION_MS
		}

		fn fetch_data(era: u64, cdn_node: &T::AccountId) -> (BytesSent, BytesReceived) {
			info!("[DAC Validator] DAC Validator is running. Current era is {}", era);
			// Todo: handle the error
			let bytes_sent_query = Self::get_bytes_sent_query_url(era, cdn_node);
			let bytes_sent_res: RedisFtAggregate = Self::http_get_json(&bytes_sent_query).unwrap();
			info!("[DAC Validator] Bytes sent sum is fetched: {:?}", bytes_sent_res);
			let bytes_sent = BytesSent::new(bytes_sent_res);

			// Todo: handle the error
			let bytes_received_query = Self::get_bytes_received_query_url(era, cdn_node);
			let bytes_received_res: RedisFtAggregate =
					Self::http_get_json(&bytes_received_query).unwrap();
			info!("[DAC Validator] Bytes received sum is fetched:: {:?}", bytes_received_res);
			let bytes_received = BytesReceived::new(bytes_received_res);

			(bytes_sent, bytes_received)
		}

		fn get_bytes_sent_query_url(era: u64, cdn_node: &T::AccountId) -> String {
			format!("{}FT.AGGREGATE/ddc:dac:searchCommonIndex/@era:[{}%20{}]/GROUPBY/2/{:?}/@era/REDUCE/SUM/1/@bytesSent/AS/bytesSentSum", DATA_PROVIDER_URL, era, era, *cdn_node)
		}

		fn get_bytes_received_query_url(era: u64, cdn_node: &T::AccountId) -> String {
			format!("{}FT.AGGREGATE/ddc:dac:searchCommonIndex/@era:[{}%20{}]/GROUPBY/2/{:?}/@era/REDUCE/SUM/1/@bytesReceived/AS/bytesReceivedSum", DATA_PROVIDER_URL, era, era, *cdn_node)
		}

		fn http_get_json<OUT: DeserializeOwned>(url: &str) -> ResultStr<OUT> {
			let body = Self::http_get_request(url).map_err(|err| {
					error!("[DAC Validator] Error while getting {}: {:?}", url, err);
					"HTTP GET error"
			})?;

			let parsed = serde_json::from_slice(&body).map_err(|err| {
					warn!("[DAC Validator] Error while parsing JSON from {}: {:?}", url, err);
					"HTTP JSON parse error"
			});

			parsed
		}

		fn http_get_request(http_url: &str) -> Result<Vec<u8>, http::Error> {
			info!("[DAC Validator] Sending request to: {:?}", http_url);

			// Initiate an external HTTP GET request. This is using high-level wrappers from
			// `sp_runtime`.
			let request = http::Request::get(http_url);

			let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(HTTP_TIMEOUT_MS));

			let pending = request.deadline(deadline).send().map_err(|_| http::Error::IoError)?;

			let response = pending.try_wait(deadline).map_err(|_| http::Error::DeadlineReached)??;

			if response.code != 200 {
					warn!("[DAC Validator] http_get_request unexpected status code: {}", response.code);
					return Err(http::Error::Unknown)
			}

			// Next we fully read the response body and collect it to a vector of bytes.
			Ok(response.body().collect::<Vec<u8>>())
		}	

		fn validate(bytes_sent: BytesSent, bytes_received: BytesReceived) -> bool {
			return if bytes_sent.sum == bytes_received.sum {
					true
			} else {
					false
			}
		}

		/// Fetch the tasks related to current validator
		fn fetch_tasks(validator: &T::AccountId) -> Vec<T::AccountId> {
			let mut cdn_nodes: Vec<T::AccountId> = vec![];
			for (cdn_id, cdn_tasks) in <Tasks<T>>::iter() {
				for decision in cdn_tasks.iter() {
					if decision.validator == *validator {
						cdn_nodes.push(cdn_id);
						break
					}
				}
			}
			cdn_nodes
		}

		fn choose(total: u32) -> Option<u32> {
			if total == 0 {
				return None
			}
			let mut random_number = Self::generate_random_number(0);

			// Best effort attempt to remove bias from modulus operator.
			for i in 1..128 {
				if random_number < u32::MAX - u32::MAX % total {
					break
				}

				random_number = Self::generate_random_number(i);
			}

			Some(random_number % total)
		}

		fn generate_random_number(seed: u32) -> u32 {
			let (random_seed, _) = <T as pallet::Config>::Randomness::random(&(b"ddc-validator", seed).encode());
			let random_number = <u32>::decode(&mut random_seed.as_ref())
				.expect("secure hashes should always be bigger than u32; qed");

			random_number
		}
	}
}