#![cfg_attr(not(feature = "std"), no_std)]

pub use frame_support::{pallet_prelude::*, parameter_types, weights::Weight, BoundedVec};
pub use frame_system::pallet_prelude::*;
pub use pallet::*;
pub use pallet_ddc_staking::{self as ddc_staking};
pub use pallet_staking::{self as staking};
pub use sp_std::prelude::*;

parameter_types! {
	pub DdcValidatorsQuorumSize: u32 = 3;
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum MethodKind {
	ProofOfDelivery,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Decision<AccountId> {
	pub decision: Option<bool>,
	pub method: MethodKind,
	pub validator: AccountId,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_staking::Config + ddc_staking::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
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
	#[pallet::getter(fn global_era_counter)]
	pub type GlobalEraCounter<T: Config> = StorageValue<_, u32>;

	#[pallet::storage]
	#[pallet::getter(fn last_managed_era)]
	pub type LastManagedEra<T: Config> = StorageValue<_, u32>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		TasksAssigned,
	}

	#[pallet::error]
	pub enum Error<T> {
		NoneValue,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(100_000)]
		pub fn inc_era(origin: OriginFor<T>) -> DispatchResult {
			ensure_signed(origin)?;
			if let Some(era) = <GlobalEraCounter<T>>::get() {
				let new_era = era.checked_add(1).unwrap_or_default();
				<GlobalEraCounter<T>>::put(new_era);
			} else {
				<GlobalEraCounter<T>>::put(1);
			}
			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(now: T::BlockNumber) -> Weight {
			if let Some(global_era_counter) = <GlobalEraCounter<T>>::get() {
				if let Some(last_managed_era) = <LastManagedEra<T>>::get() {
					if last_managed_era >= global_era_counter {
						log::info!("Task manager | block: {:?}, waiting for new era", now);
						return 0
					}
					<LastManagedEra<T>>::put(global_era_counter);
				} else {
					<LastManagedEra<T>>::put(global_era_counter);
				}
			} else {
				log::info!("Task manager | block: {:?}, waiting for new era", now);
				return 0
			}

			log::info!("Task manager | block: {:?}, producing new assignment", now);

			let validators: Vec<T::AccountId> = <staking::Validators<T>>::iter_keys().collect();
			log::info!("Task manager | block: {:?}, validators: {:?}", now, validators);

			let edges: Vec<T::AccountId> = <ddc_staking::pallet::Edges<T>>::iter_keys().collect();
			log::info!("Task manager | block: {:?}, edges: {:?}", now, edges);

			for edge in edges {
				let decisions: BoundedVec<Decision<T::AccountId>, DdcValidatorsQuorumSize> =
					Default::default();
				<Tasks<T>>::insert(&edge, decisions);
			}

			Self::deposit_event(Event::<T>::TasksAssigned);

			0
		}
	}

	impl<T: Config> Pallet<T> {
		/// Fetch the tasks related to current validator
		fn fetch_tasks(validator: T::AccountId) -> Vec<T::AccountId> {
			let mut cdn_nodes: Vec<T::AccountId> = vec![];
			for (cdn_id, cdn_tasks) in <Tasks<T>>::iter() {
				for decision in cdn_tasks.iter() {
					if decision.validator == validator {
						cdn_nodes.push(cdn_id);
						break;
					}
				}
			}
			cdn_nodes
		}
	}
}
