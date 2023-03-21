#![cfg_attr(not(feature = "std"), no_std)]

use alloc::{format, string::String};
use alt_serde::{de::DeserializeOwned, Deserialize, Serialize};
use codec::{Encode, Decode, MaxEncodedLen, HasCompact};
use frame_support::{
    decl_event, decl_module, decl_storage,
    log::{error, info, warn},
    traits::Currency,
    weights::{Weight},
    dispatch::DispatchResult,
    RuntimeDebug,
    BoundedVec,
    parameter_types,
};
use frame_system::{ensure_signed, offchain::{CreateSignedTransaction, Signer, SigningTypes, AppCrypto, SendSignedTransaction}};
use pallet_staking::{self as staking};
use sp_core::crypto::{KeyTypeId, UncheckedFrom};use sp_io::offchain::timestamp;
use sp_runtime::offchain::{http, Duration};
use sp_std::{vec, vec::Vec};
extern crate alloc;
use pallet_session as session;

parameter_types! {
	pub DdcValidatorsQuorumSize: u32 = 3;
}

use core::fmt::Debug;
use scale_info::TypeInfo;

type BalanceOf<T> = <<T as pallet_contracts::Config>::Currency as Currency<
    <T as frame_system::Config>::AccountId,
>>::Balance;

pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"dacv");

pub const HTTP_TIMEOUT_MS: u64 = 30_000;

const TIME_START_MS: u64 = 1_672_531_200_000;
const ERA_DURATION_MS: u64 = 120_000;
const ERA_IN_BLOCKS: u8 = 20;

const DATA_PROVIDER_URL: &str = "http://localhost:7379/";

type ResultStr<T> = Result<T, &'static str>;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(crate = "alt_serde")]
#[serde(rename_all = "camelCase")]
pub struct RedisFtAggregate {
    #[serde(rename = "FT.AGGREGATE")]
    pub ft_aggregate: (u32, Vec<String>, Vec<String>),

    // This struct should be correct for any length but there is error while parsing JSON
    // #[serde(rename = "FT.AGGREGATE")]
    // pub ft_aggregate: Vec<FtAggregate>,
}

// #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// #[serde(crate = "alt_serde")]
// pub enum FtAggregate {
//     Length(u32),
//     Node(Vec<String>),
// }

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

// impl BytesSent {
//     pub fn new(aggregate: RedisFtAggregate) -> BytesSent {
//         // let (_, values, values2) = aggregate.ft_aggregate;
//         let data = aggregate.ft_aggregate[1].clone();
//
//         match data {
//             FtAggregate::Node(node) => {
//                 return BytesSent {
//                     node_public_key: node[1].clone(),
//                     era: node[3].clone(),
//                     sum: node[5].parse::<u32>().expect("bytesSentSum must be convertable to u32"),
//                 }
//             }
//             FtAggregate::Length(_) => panic!("[DAC Validator] Not a Node"),
//         }
//     }
// }

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

// impl BytesReceived {
//     pub fn new(aggregate: RedisFtAggregate) -> BytesReceived {
//         // let (_, values, values2) = aggregate.ft_aggregate;
//
//         let data = aggregate.ft_aggregate[1].clone();
//
//         match data {
//             FtAggregate::Node(node) => {
//                 return BytesReceived {
//                     node_public_key: node[1].clone(),
//                     era: node[3].clone(),
//                     sum: node[5].parse::<u32>().expect("bytesReceivedSum must be convertable to u32"),
//                 }
//             }
//             FtAggregate::Length(_) => panic!("[DAC Validator] Not a Node"),
//         }
//     }
// }

#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo, Default)]
pub struct ValidationResult<AccountId> {
    era: String,
    signer: AccountId,
    val_res: bool,
    cdn_node_pub_key: String,
}

// use sp_std::fmt;
// impl<T: frame_system::Config> fmt::Display for ValidationResult<T::BlockNumber, T::AccountId>
// {
// 	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
// 		write!(f, "ValidationResult signer {}", self.signer)
// 	}
// }

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

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

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
    + CreateSignedTransaction<Call<Self>>
        where
            <Self as frame_system::Config>::AccountId: AsRef<[u8]> + UncheckedFrom<Self::Hash>,
            <BalanceOf<Self> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
    {
        /// The overarching dispatch call type.
        type Call: From<Call<Self>>;

        type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        <T as frame_system::Config>::AccountId: AsRef<[u8]> + UncheckedFrom<T::Hash>,
        <BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
    {
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

            // ValidationResults::<T>::append(cur_validation);
            // ValidationResults::<T>::mutate(|results| {
            //     results.push(cur_validation);
            //
            //     results.clone()
            // });
            v_results.push(cur_validation);

            ValidationResults::<T>::set(v_results);

            Ok(())
        }
    }

    #[pallet::storage]
    #[pallet::getter(fn validation_results)]
    pub(super) type ValidationResults<T: Config> = StorageValue<_, Vec<ValidationResult::<T::AccountId>>, ValueQuery>;
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
        let (bytes_sent, bytes_received) = Self::fetch_data(current_era);
        let val_res = Self::validate(bytes_sent.clone(), bytes_received.clone());

        let cdn_node_pub_key = bytes_sent.node_public_key.clone();
        let tx_res = signer.send_signed_transaction(|_acct| {
            info!("[DAC Validator] Sending save_validated_data tx");

            // This is the on-chain function
            Call::save_validated_data { val_res, cdn_node_pub_key: cdn_node_pub_key.clone(), era: bytes_sent.era.clone() }
        });

        match &tx_res {
            None | Some((_, Err(()))) => {
                return Err("Error while submitting save_validated_data TX")
            }
            Some((_, Ok(()))) => {}
        }

        // info!("[DAC Validator] save_validated_data: {:?}", ValidationResults::<T>::get());

        Ok(())
    }

    fn get_signer() -> ResultStr<Signer<T, T::AuthorityId>> {
        let signer = Signer::<_, _>::any_account();
        if !signer.can_sign() {
            return Err("[DAC Validator] No local accounts available. Consider adding one via `author_insertKey` RPC.");
        }

        Ok(signer)
    }

    fn validate(bytes_sent: BytesSent, bytes_received: BytesReceived) -> bool {
        return if bytes_sent.sum == bytes_received.sum {
            true
        } else {
            false
        }
    }

    fn fetch_data(era: u64 ) -> (BytesSent, BytesReceived){
        info!("[DAC Validator] DAC Validator is running. Current era is {}", era);
        // Todo: handle the error
        let bytes_sent_query = Self::get_bytes_sent_query_url(era);
        let bytes_sent_res: RedisFtAggregate = Self::http_get_json(&bytes_sent_query).unwrap();
        info!("[DAC Validator] Bytes sent sum is fetched: {:?}", bytes_sent_res);
        let bytes_sent = BytesSent::new(bytes_sent_res);

        // Todo: handle the error
        let bytes_received_query = Self::get_bytes_received_query_url(era);
        let bytes_received_res: RedisFtAggregate =
            Self::http_get_json(&bytes_received_query).unwrap();
        info!("[DAC Validator] Bytes received sum is fetched:: {:?}", bytes_received_res);
        let bytes_received = BytesReceived::new(bytes_received_res);

        (bytes_sent, bytes_received)
    }

    fn get_current_era() -> u64 {
        // info!("timestamp: {}", timestamp().unix_millis());
        // info!("TIME_START_MS: {}", TIME_START_MS);
        // info!("ERA_DURATION_MS: {}", ERA_DURATION_MS);
        // info!("1: {}", timestamp().unix_millis() - TIME_START_MS);
        // info!("2: {}", (timestamp().unix_millis() - TIME_START_MS) % ERA_DURATION_MS);

        (timestamp().unix_millis() - TIME_START_MS) / ERA_DURATION_MS
    }

    fn get_bytes_sent_query_url(era: u64) -> String {
        format!("{}FT.AGGREGATE/ddc:dac:searchCommonIndex/@era:[{}%20{}]/GROUPBY/2/@nodePublicKey/@era/REDUCE/SUM/1/@bytesSent/AS/bytesSentSum", DATA_PROVIDER_URL, era, era)
    }

    fn get_bytes_received_query_url(era: u64) -> String {
        format!("{}FT.AGGREGATE/ddc:dac:searchCommonIndex/@era:[{}%20{}]/GROUPBY/2/@nodePublicKey/@era/REDUCE/SUM/1/@bytesReceived/AS/bytesReceivedSum", DATA_PROVIDER_URL, era, era)
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

    fn validators() -> Vec<<T as session::Config>::ValidatorId> {
        <pallet_session::Pallet<T>>::validators()
    }

    /// Reward a validator.
    pub fn reward_by_ids(
        validators_points: impl IntoIterator<Item = (T::AccountId, u32)>,
    ) -> Result<(), ()> {
        <staking::Pallet<T>>::reward_by_ids(validators_points);
        Ok(())
    }
}