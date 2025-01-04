use frame_support::{
	derive_impl, 
	weights::{
		constants,
		Weight,
		WeightToFeePolynomial,
		WeightToFeeCoefficients,
		ConstantMultiplier,
		WeightToFeeCoefficient,
		RuntimeDbWeight,
		constants::WEIGHT_REF_TIME_PER_SECOND,
	},
	PalletId,
	parameter_types,
	ConsensusEngineId,
	traits::{ 
		AsEnsureOriginWithArg,
		OnUnbalanced,
		fungible::Credit,
		Imbalance,
		tokens::imbalance::ResolveTo,
		fungible::Balanced,
	},
};
use frame_system::{
	EnsureRoot, GenesisConfig,
	EnsureWithSuccess, EnsureSigned,
	limits::{BlockLength, BlockWeights},
};
use sp_runtime::{
	generic, impl_opaque_keys, traits:: { AccountIdConversion, BlakeTwo256, ConstU32, ConstU64, ConstU8}, BuildStorage, MultiAddress, MultiSignature
};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use xcm::latest::prelude::BodyId;
use sp_runtime::Perbill;
use smallvec::smallvec;
use polkadot_runtime_common::{
	SlowAdjustingFeeUpdate,
	BlockHashCount,
};
use frame_support::traits::VariantCountOf;
use frame_support::pallet_prelude::DispatchClass;
use sp_runtime::Percent;
use sp_runtime::traits::Zero;

pub const SLOT_DURATION: u64 = 6000;
pub type Balance = u128;
pub type AccountId = u64;
pub type BlockNumber = u32;

pub const MILLI_SECS_PER_BLOCK: u32 = 6000;
pub const MINUTES: BlockNumber = 60_000 / (MILLI_SECS_PER_BLOCK as BlockNumber);
pub const MILLI_UNIT: Balance = 1_000_000_000;
pub const MICRO_UNIT: Balance = 1_000_000;

pub type SignedExtra = (
	frame_system::CheckNonZeroSender<Test>,
	frame_system::CheckSpecVersion<Test>,
	frame_system::CheckTxVersion<Test>,
	frame_system::CheckGenesis<Test>,
	frame_system::CheckEra<Test>,
	frame_system::CheckNonce<Test>,
	frame_system::CheckWeight<Test>,
	pallet_transaction_payment::ChargeTransactionPayment<Test>,
);

pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

pub type Hash = sp_core::H256;
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
pub type Address = MultiAddress<AccountId, ()>;
pub type Signature = MultiSignature;

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
	pub type Timestamp = pallet_timestamp;


	#[runtime::pallet_index(2)]
	pub type Balances = pallet_balances;
	#[runtime::pallet_index(3)]
	pub type TransactionPayment = pallet_transaction_payment;

	#[runtime::pallet_index(4)]
	pub type Authorship = pallet_authorship;
	#[runtime::pallet_index(5)]
	pub type CollatorSelection = pallet_collator_selection;
	#[runtime::pallet_index(6)]
	pub type Session = pallet_session;
	#[runtime::pallet_index(7)]
	pub type Aura = pallet_aura;

	#[runtime::pallet_index(8)]
	pub type Assets = pallet_assets;
	#[runtime::pallet_index(9)]
	pub type AssetRate = pallet_asset_rate;
	#[runtime::pallet_index(10)]
	pub type Indices = pallet_indices;
	#[runtime::pallet_index(11)]
	pub type Treasury = pallet_treasury;

	#[runtime::pallet_index(12)]
	pub type XodeStaking = crate;
}

const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(5);
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
const MAXIMUM_BLOCK_WEIGHT: Weight = Weight::from_parts(
	WEIGHT_REF_TIME_PER_SECOND.saturating_mul(2),
	cumulus_primitives_core::relay_chain::MAX_POV_SIZE as u64,
);

parameter_types! {
	pub const BlockExecutionWeight: Weight = Weight::from_parts(constants::WEIGHT_REF_TIME_PER_NANOS.saturating_mul(5_000_000), 0);
	pub const ExtrinsicBaseWeight: Weight = Weight::from_parts(constants::WEIGHT_REF_TIME_PER_NANOS.saturating_mul(125_000), 0);
	pub const RocksDbWeight: RuntimeDbWeight = RuntimeDbWeight {
		read: 25_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
		write: 100_000 * constants::WEIGHT_REF_TIME_PER_NANOS,
	};
}

parameter_types! {
	pub RuntimeBlockLength: BlockLength =
		BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
		.base_block(BlockExecutionWeight::get())
		.for_class(DispatchClass::all(), |weights| {
			weights.base_extrinsic = ExtrinsicBaseWeight::get();
		})
		.for_class(DispatchClass::Normal, |weights| {
			weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
		})
		.for_class(DispatchClass::Operational, |weights| {
			weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
			weights.reserved = Some(
				MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT
			);
		})
		.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
	 	.build_or_panic();
	pub const SS58Prefix: u16 = 42;
}


#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = RuntimeBlockWeights;
    type BlockLength = RuntimeBlockLength;
    type DbWeight = RocksDbWeight;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Hash = Hash;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type BlockHashCount = BlockHashCount;
	type Block = Block;
    type PalletInfo = PalletInfo;
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
	type MaxLocks = ConstU32<50>;
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Test>;
	type MaxReserves = ConstU32<50>;
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
}

pub const ASSETS_UNIT: Balance = 1_000_000_000_000;
pub const ASSETS_MILLIUNIT: Balance = 1_000_000_000;
pub const ASSETS_MICROUNIT: Balance = 1_000_000;
pub const ASSETS_EXISTENTIAL_DEPOSIT: Balance = ASSETS_MILLIUNIT;

pub const fn deposit(items: u32, bytes: u32) -> Balance {
	(items as Balance * 20 * ASSETS_UNIT + (bytes as Balance) * 100 * ASSETS_MICROUNIT) / 100
}

parameter_types! {
	pub const AssetDeposit: Balance = 10 * ASSETS_UNIT;
	pub const AssetAccountDeposit: Balance = deposit(1, 16);
	pub const ApprovalDeposit: Balance = ASSETS_EXISTENTIAL_DEPOSIT;
	pub const StringLimit: u32 = 50;
	pub const MetadataDepositBase: Balance = deposit(1, 68);
	pub const MetadataDepositPerByte: Balance = deposit(0, 1);
}

impl pallet_assets::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type RemoveItemsLimit = ConstU32<1_000>;
	type AssetId = u32;
	type AssetIdParameter = codec::Compact<u32>;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type AssetDeposit = AssetDeposit;
	type AssetAccountDeposit = AssetAccountDeposit;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = StringLimit;
	type Freezer = ();
	type Extra = ();
	type CallbackHandle = ();
	type WeightInfo = pallet_assets::weights::SubstrateWeight<Test>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

impl pallet_asset_rate::Config for Test {
	type CreateOrigin = frame_system::EnsureRoot<AccountId>;
	type RemoveOrigin = frame_system::EnsureRoot<AccountId>;
	type UpdateOrigin = frame_system::EnsureRoot<AccountId>;
	type Currency = Balances;
	type AssetKind = u32;
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_asset_rate::weights::SubstrateWeight<Test>;
}

pub const MILLICENTS: Balance = 1_000_000_000;
pub const CENTS: Balance = 1_000 * MILLICENTS; 
pub const DOLLARS: Balance = 100 * CENTS;

parameter_types! {
	pub const IndexDeposit: Balance = 1 * DOLLARS;
}

impl pallet_indices::Config for Test {
	type AccountIndex = u32;
	type Currency = Balances;
	type Deposit = IndexDeposit;
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = pallet_indices::weights::SubstrateWeight<Test>;
}

parameter_types! {
    pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub const SpendPeriod: BlockNumber = 1 * MINUTES; 
	pub const MaxApprovals: u32 = 100;
	pub const MaxBalance: Balance = Balance::max_value();
	pub XodeTreasuryAccount: AccountId = TreasuryPalletId::get().into_account_truncating();
	pub const SpendPayoutPeriod: BlockNumber = 30 * MINUTES; 
}

impl pallet_treasury::Config for Test {
    type PalletId = TreasuryPalletId;
    type Currency = Balances;
    type RejectOrigin = frame_system::EnsureRoot<AccountId>;
    type RuntimeEvent = RuntimeEvent; 
    type SpendPeriod = SpendPeriod;
    type Burn = ();  
	type BurnDestination = ();
	type SpendFunds = ();  
    type WeightInfo = ();
    type MaxApprovals = ConstU32<100>;
	type AssetKind = u32;
	type Beneficiary = AccountId;
	type BeneficiaryLookup = pallet_indices::Pallet<Test>;
	type SpendOrigin = EnsureWithSuccess<frame_system::EnsureRoot<AccountId>, AccountId, MaxBalance>; 
	type Paymaster = frame_support::traits::tokens::pay::PayAssetFromAccount<pallet_assets::Pallet<Test>, XodeTreasuryAccount>;
	type BalanceConverter = pallet_asset_rate::Pallet<Test>;
	type PayoutPeriod = SpendPayoutPeriod;
}

pub struct WeightToFee;
impl WeightToFeePolynomial for WeightToFee {
	type Balance = Balance;
	fn polynomial() -> WeightToFeeCoefficients<Self::Balance> {
		println!("Polynomial: {:?}", Balance::from(ExtrinsicBaseWeight::get().ref_time()));
		let p = MILLI_UNIT / 10;
		let q = 100 * Balance::from(ExtrinsicBaseWeight::get().ref_time());
		smallvec![WeightToFeeCoefficient {
			degree: 1,
			negative: false,
			coeff_frac: Perbill::from_rational(p % q, q),
			coeff_integer: p / q,
		}]
	}
}

pub const TREASURY_SHARE: u32 = 20;
pub const AUTHOR_SHARE: u32 = 80;

pub struct DealWithFees<R>(core::marker::PhantomData<R>);
impl<R> OnUnbalanced<Credit<R::AccountId, pallet_balances::Pallet<R>>> for DealWithFees<R>
where
	R: pallet_balances::Config + pallet_authorship::Config + pallet_treasury::Config + crate::Config,
    <R as frame_system::Config>::AccountId: From<AccountId>,
    <R as frame_system::Config>::AccountId: Into<AccountId>,
{
	fn on_unbalanceds(
		mut fees_then_tips: impl Iterator<Item = Credit<R::AccountId, pallet_balances::Pallet<R>>>,
	) {
		println!("Fee treasury");
		if let Some(fees) = fees_then_tips.next() {
			let mut split = fees.ration(TREASURY_SHARE, AUTHOR_SHARE);
			if let Some(tips) = fees_then_tips.next() {
				tips.merge_into(&mut split.1);
			}
			ResolveTo::<pallet_treasury::TreasuryAccountId<R>, pallet_balances::Pallet<R>>::on_unbalanced(split.0);
			<ToAuthor<R> as OnUnbalanced<_>>::on_unbalanced(split.1);
		}
	}
}

pub struct ToAuthor<R>(core::marker::PhantomData<R>);
impl<R> OnUnbalanced<Credit<R::AccountId, pallet_balances::Pallet<R>>> for ToAuthor<R>
where
    R: pallet_balances::Config + pallet_authorship::Config + crate::Config,
    <R as frame_system::Config>::AccountId: From<AccountId>,
    <R as frame_system::Config>::AccountId: Into<AccountId>,
{
    fn on_nonzero_unbalanced(
        amount: Credit<<R as frame_system::Config>::AccountId, pallet_balances::Pallet<R>>,
    ) {
		println!("Author fee - 1");
        if let Some(author) = <pallet_authorship::Pallet<R>>::author() {

			println!("Author fee - 2");
			if let Some(candidate) = crate::ProposedCandidates::<R>::get().iter().find(|c| c.who == author) {

				// commission of the delegator
				println!("Author fee - 3");
				let commission = Percent::from_percent(candidate.commission.into());
				if commission.deconstruct() > 0 {

					println!("Author fee - 4");
					if let Some(delegations) = crate::Delegations::<R>::get(&author) {
						
						// remaining amount to be shared
						let mut remaining_amount = amount;

						// distribute only to delegator with stake > 0
						println!("Remaining Amount: {:?}", remaining_amount);
						for (_index, delegation) in delegations.iter().enumerate() {
							if delegation.stake > Zero::zero() {
								
								// stake / total_stake
								print!("Delegator: {:?}, ", delegation.delegator);
								let delegator_share_ratio = Percent::from_rational(delegation.stake, candidate.total_stake);
								print!("Ratio: {:?}, ", delegator_share_ratio);
								print!("Commission: {:?}, ", commission);

								// delegator_share_amount
								//let delegator_share_amount = shared_amount.saturating_mul(
								//	(delegator_share_ratio.deconstruct() / 100).into()
								//).saturating_mul(
								//	(commission.deconstruct() / 100).into()
								//);
								let ds1 = Perbill::from_percent(delegator_share_ratio.deconstruct() as u32).mul_ceil(remaining_amount.peek());
								let ds2 = Perbill::from_percent(commission.deconstruct() as u32).mul_ceil(ds1);
								print!("Delegator share: {:?}, ", ds2);


								// extract the delegator_share_amount (share) from the remaining amount, then resolve it.
								let (share, leftover) = remaining_amount.split(ds2.into());
								let _ = <pallet_balances::Pallet<R>>::resolve(&delegation.delegator,share,);

								// the remaining amount (left-over) will be given to the next delegator.
								remaining_amount = leftover; 
								println!("Remaining amount: {:?}, ", remaining_amount);
							}
						}

						// after all its delegator has been paid, the remaining amount will given to the author
						let _ = <pallet_balances::Pallet<R>>::resolve(&author, remaining_amount);
					}
				} else {
					// if the commission is zero, the author will not share
					let _ = <pallet_balances::Pallet<R>>::resolve(&author, amount);
				}
			} else {
				// if there is no delegator the author will get everything
				let _ = <pallet_balances::Pallet<R>>::resolve(&author, amount);
			}
        }
    }
}

parameter_types! {
	pub const TransactionByteFee: Balance = 10 * MICRO_UNIT;
}

impl pallet_transaction_payment::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, DealWithFees<Test>>;
	type WeightToFee = WeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
	type OperationalFeeMultiplier = ConstU8<5>;
}

parameter_types! {
	pub const XodeStakingPalletId: PalletId = PalletId(*b"xd/stkng");
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
	type MaxProposedCandidates = MaxProposedCandidates;
	type MaxProposedCandidateDelegates = MaxProposedCandidateDelegates;
	type XaverNodes = Nodes;
	type StakingCurrency = Balances;
	type WeightInfo = crate::weights::SubstrateWeight<Test>;
	type PalletId = XodeStakingPalletId;
}

pub fn test1_ext() -> sp_io::TestExternalities {
	GenesisConfig::<Test>::default().build_storage().unwrap().into()
}