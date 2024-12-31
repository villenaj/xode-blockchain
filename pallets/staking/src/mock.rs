use frame_support::{derive_impl, 
	weights::constants::RocksDbWeight,
	PalletId,
};
use frame_system::{mocking::MockBlock, EnsureRoot, GenesisConfig};
use sp_runtime::{impl_opaque_keys, traits::ConstU64, BuildStorage, };
use frame_support::parameter_types;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use xcm::latest::prelude::BodyId;

use frame_support::ConsensusEngineId;

pub const SLOT_DURATION: u64 = 6000;
pub type Balance = u128;
pub type AccountId = u64;
pub type BlockNumber = u32;

pub const MILLI_SECS_PER_BLOCK: u32 = 6000;
pub const MINUTES: BlockNumber = 60_000 / (MILLI_SECS_PER_BLOCK as BlockNumber);

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: Aura,
	}
}

// Configure a mock runtime to test the pallet.
#[frame_support::runtime]
mod test_runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;

	#[runtime::pallet_index(1)]
	pub type XodeStaking = crate;

	#[runtime::pallet_index(2)]
	pub type Balances = pallet_balances;

	#[runtime::pallet_index(3)]
	pub type Timestamp = pallet_timestamp;

	#[runtime::pallet_index(4)]
	pub type Aura = pallet_aura;

	#[runtime::pallet_index(5)]
	pub type CollatorSelection = pallet_collator_selection;

	#[runtime::pallet_index(6)]
	pub type Authorship = pallet_authorship;

	#[runtime::pallet_index(7)]
	pub type Session = pallet_session;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Nonce = u64;
	type Block = MockBlock<Test>;
	type BlockHashCount = ConstU64<250>;
	type DbWeight = RocksDbWeight;
	type AccountData = pallet_balances::AccountData<Balance>;
}

parameter_types! {
	pub const MinimumPeriod: u64 = 0;
}
impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = Aura;
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

parameter_types! {
	pub const MaxAuthorities: u32 = 1_000;
	pub const AllowMultipleBlocksPerSlot: bool = true;
}
impl pallet_aura::Config for Test {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = MaxAuthorities;
	type AllowMultipleBlocksPerSlot = AllowMultipleBlocksPerSlot;
	type SlotDuration = ConstU64<SLOT_DURATION>;
}

parameter_types! {
	pub const PotId: PalletId = PalletId(*b"PotStake");
	pub const SessionLength: BlockNumber = MINUTES;
	pub const StakingAdminBodyId: BodyId = BodyId::Defense;
	pub const MaxCandidates: u32 = 100;
	pub const MinEligibleCollators: u32 = 4;
	pub const MaxInvulnerables: u32 = 100;
}

impl pallet_collator_selection::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type UpdateOrigin = EnsureRoot<AccountId>;
	type PotId = PotId;
	type MaxCandidates = MaxCandidates;
	type MinEligibleCollators = MinEligibleCollators;
	type MaxInvulnerables = MaxInvulnerables;
	type KickThreshold = Period;
	type ValidatorId = AccountId;
	type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
	type ValidatorRegistration = Session;
	type WeightInfo = ();
}

parameter_types! {
	pub const Period: u32 = MINUTES;
	pub const Offset: u32 = 0;
}

impl pallet_session::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = AccountId;
	type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
	type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
	type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
	type SessionManager = XodeStaking;
	type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
	type Keys = SessionKeys;
	type WeightInfo = ();
}


pub struct AuthorGiven;
static mut FIXED_AUTHOR: Option<AccountId> = None;

impl frame_support::traits::FindAuthor<AccountId> for AuthorGiven {
    fn find_author<'a, I>(_digests: I) -> Option<AccountId>
    where
        I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
    {
		unsafe {
            let author = FIXED_AUTHOR;
            println!("Get author(r): {:?}", author);
            author
        }
    }	
}
impl AuthorGiven {
    pub fn set_author(author: AccountId) {
        unsafe {
            FIXED_AUTHOR = Some(author);
            println!("Set author: {:?}", author);
        }
    }

    pub fn clear_author() {
        unsafe {
            FIXED_AUTHOR = None;
            println!("Clear author");
        }
    }
}

impl pallet_authorship::Config for Test {
	//type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
	type FindAuthor = AuthorGiven;
	type EventHandler = (CollatorSelection, XodeStaking);
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 1;
}
impl pallet_balances::Config for Test {
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 4];
	type MaxLocks = ();
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type RuntimeHoldReason = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeFreezeReason = ();
}

parameter_types! {
	pub const MaxProposedCandidates: u32 = 200;
	pub const MaxProposedCandidateDelegates: u32 = 200;
	pub const Nodes: &'static [&'static str] = &[
		"0x306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20",  	// Charlie
		"0x90b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22", 	// Dave 
		"0xe659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e",   // Eve 
	];
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MaxProposedCandidates = MaxProposedCandidates;
	type MaxProposedCandidateDelegates = MaxProposedCandidateDelegates;
	type XaverNodes = Nodes;
	type StakingCurrency = Balances;
}

pub fn test1_ext() -> sp_io::TestExternalities {
	GenesisConfig::<Test>::default().build_storage().unwrap().into()
}