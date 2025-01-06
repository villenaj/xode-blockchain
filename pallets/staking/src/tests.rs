use crate::{mock::*, Error, 
	CandidateInfo, Status,
	DesiredCandidates, ProposedCandidates, WaitingCandidates,
};
use codec::Encode;
use frame_support::{
	assert_noop, assert_ok, dispatch::GetDispatchInfo, traits::{Hooks, OnUnbalanced,}
};

use pallet_session::SessionManager;
use frame_support::traits::Currency;
use pallet_transaction_payment::{ChargeTransactionPayment, OnChargeTransaction};
use sp_core::sr25519;
use sp_runtime::traits::{Dispatchable, SignedExtension};

use pallet_transaction_payment::FungibleAdapter;

fn check_staking_storages(
	number_of_authorities: usize,
	number_of_invulnerables: usize,
	number_of_queued_keys: usize,
	number_of_proposed_candidates: usize,
	number_of_waiting_candidates: usize,
	scene: usize,
	scene_step: usize,
) {
	println!("Scene {:?}-{:?}",scene, scene_step);

	let authorities = pallet_aura::Authorities::<Test>::get();
	println!("Authorities {:?}",authorities);
	assert_eq!(authorities.len(), number_of_authorities);
	
	let invulnerables = pallet_collator_selection::Invulnerables::<Test>::get();
	println!("Invulnerables {:?}",invulnerables);
	assert_eq!(invulnerables.len(), number_of_invulnerables);	

	let queued_keys = pallet_session::QueuedKeys::<Test>::get();
	println!("Keys {:?}",queued_keys);
	assert_eq!(queued_keys.len(), number_of_queued_keys);

	let proposed_candidates = ProposedCandidates::<Test>::get();
	println!("Proposed Candidates {:?}",proposed_candidates);
	assert_eq!(proposed_candidates.len(), number_of_proposed_candidates);

	let waiting_candidates = WaitingCandidates::<Test>::get();
	println!("Waiting Candidates {:?}",waiting_candidates);
	assert_eq!(waiting_candidates.len(), number_of_waiting_candidates);
}

fn set_new_block_with_author(
	candidate: &mut CandidateInfo<AccountId, Balance, BlockNumber>,
	block_number: BlockNumber
) {
	AuthorGiven::set_author(candidate.who);
	Authorship::on_initialize(System::block_number());
	Authorship::on_finalize(System::block_number());

	System::set_block_number((block_number).into());
	XodeStaking::on_initialize(System::block_number());
	candidate.last_authored = System::block_number();
}

#[test]
fn test_pallet_xode_staking_process() {
	test1_ext().execute_with(|| {
		// ========================================================================
		// SCENE 1 (Initialization): At Block 0 and Session 1 initialization
		// ------------------------------------------------------------------------
		// 1. There are three (3) desired candidates as set in the mock runtime.
		// 2. Provide balances for the three (3) desired candidates.
		// 3. We expect them to author so we provide session keys.
		// 4. Then advance the block and session so that these desired candidates 
		//    will be sent to invulnerable in the collator selection at the same 
		//	  time queued keys and authorities are updated.
		// 5. The authorities is still 0 at Session 1 
		// ========================================================================
		System::set_block_number(0);
		XodeStaking::on_initialize(System::block_number());
		
		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");

		for i in 0..3 {
			let _ = Balances::deposit_creating(&desired_candidates[i], 1_000_000);
			let k = i as u8 + 1u8;
			let key = sr25519::Public::from_raw([k; 32]);
			let session_keys = SessionKeys { aura: key.into(),};
			let result = Session::set_keys(
				RuntimeOrigin::signed(desired_candidates[i]), 
				session_keys.clone(), 
				Vec::new()
			);
			println!("{:?} free balance: {:?}, {:?}: {:?}",desired_candidates[i], 
				Balances::free_balance(&desired_candidates[i]), 
				session_keys, 
				result
			);
			assert!(result.is_ok(), "Failed to set session keys: {:?}", result);
        }

		System::set_block_number((1 * MINUTES).into());
		XodeStaking::on_initialize(System::block_number());

		XodeStaking::new_session(1);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			0, 
			3, 
			3, 
			0, 
			3,
			1,
			1,
		);

		// =======================================================================
		// SCENE 2 (Registering): Within Session 1 and Session 2 initialization
		// -----------------------------------------------------------------------
		// 1. Register two proposed candidates (Candidate-1 and Candidate-2).
		// 2. Bond these candidates and check the free balance. 
		// 3. Check also the sorting of proposed candidates.
		// 4. At Session 2 initialization, we have 3 authorities which is taken 
		//    from Session 1.
		// =======================================================================
		let mut candidates: [CandidateInfo<AccountId, Balance, BlockNumber>; 3] = [
			Default::default(), 
			Default::default(),
			Default::default(),
		];
		for i in 0..2 {
			let account_id = i as u64 + 1;
			let candidate = CandidateInfo {
				who: account_id,
				bond: 0,
				total_stake: 0,
				last_updated: System::block_number(),
				last_authored: System::block_number(),
				leaving: false,
				offline: false,
				commission: 0,
				status: Status::Online,
				status_level: 0,
			};
			candidates[i] = candidate;
			let _ = XodeStaking::register_candidate(RuntimeOrigin::signed(account_id));
		}

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates.len(), 2, "The number of proposed candidates should be 2");

		for i in 0..2 {
			assert_eq!(proposed_candidates[i], candidates[i], "Must match");

			let _ = Balances::deposit_creating(&proposed_candidates[i].who, 1_000_000);
			let k = i as u8 + 10u8;
			let key = sr25519::Public::from_raw([k; 32]);
			let session_keys = SessionKeys { aura: key.into(),};
			let result = Session::set_keys(
				RuntimeOrigin::signed(proposed_candidates[i].who), 
				session_keys.clone(), 
				Vec::new()
			);
			assert!(result.is_ok(), "Failed to set session keys: {:?}", result);

			let b = (i as u128 + 1u128) * 100u128;
			let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(proposed_candidates[i].who), b);
			assert_eq!(Balances::free_balance(&proposed_candidates[i].who), 1_000_000u128 - b);

			candidates[i].bond = b;
			candidates[i].last_updated = System::block_number();
		}

		// Index reverse because of sorting based on bond
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[1], "Must match");
		assert_eq!(proposed_candidates[1], candidates[0], "Must match");

		set_new_block_with_author(&mut candidates[0], 2 * MINUTES);

		XodeStaking::new_session(2);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			3, 
			3, 
			3, 
			2, 
			5,
			2,
			1,
		);

		for i in 0..2 {
			candidates[i].status = Status::Waiting;
		}

		// =======================================================================
		// SCENE 3 (Bonding): Within Session 2 and Session 3 initialization
		// -----------------------------------------------------------------------
		// 1. Increase and decrease bonds of the proposed candidates.
		// 2. While decreasing the bond try incrementing the block number
		// 3. Take note of the actual authors on this session.  The proposed 
		//    candidates must author a block within Session 2 or it will be set to
		//    offline.  Todo: Slashed if not authoring.
		// =======================================================================
		let proposed_candidates = ProposedCandidates::<Test>::get();
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(proposed_candidates[1].who), 400);

		// 1_000_000 - 400 = 999_600
		assert_eq!(Balances::free_balance(&proposed_candidates[1].who), 999_600);

		candidates[0].bond = 400;
		candidates[0].last_updated = System::block_number();
		
		// Revert to original sort due to an increase of bond
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[0], "Must match");
		assert_eq!(proposed_candidates[1], candidates[1], "Must match");

		set_new_block_with_author(&mut candidates[1], System::block_number() + 1);
		
		let proposed_candidates = ProposedCandidates::<Test>::get();
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(proposed_candidates[0].who), 200);

		assert_eq!(Balances::free_balance(&proposed_candidates[0].who), 999_800);

		candidates[0].bond = 200;
		candidates[0].last_updated = System::block_number();

		// Reversal again due to the decrease of bond
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[1], "Must match");
		assert_eq!(proposed_candidates[1], candidates[0], "Must match");

		set_new_block_with_author(&mut candidates[1], 3 * MINUTES);

		XodeStaking::new_session(3);		
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			3, 
			5, 
			5, 
			2, 
			5,
			3,
			1,
		);

		// No more queuing, immediately sent to authoring at level 0
		candidates[0].status = Status::Authoring;
		candidates[1].status = Status::Authoring;

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[1], "Must match");
		assert_eq!(proposed_candidates[1], candidates[0], "Must match");
		
		// =======================================================================
		// SCENE 4 (Staking): Within Session 3 and Session 4 initialization
		// ----------------------------------------------------------------------- 
		// 1. Add stakes on the proposed candidates
		// 2. Try to increase the block and execute un-stake
		// 3. Todo: We need to check if one of the proposed candidates did not
		//          author a block, we cannot assume!
		// 4. Todo: After un-staking (Separate Test)
		//			test_pallet_xode_staking_unstaked()
		//		4.1. Unreserved the balance
		//		4.2. If the stake is zero, remove the delegation
		//		4.3. Stake the same candidate again
		// =======================================================================
		for i in 11..14 {
			let amount = (i as u128 - 10) * 1_000;
			let _ = Balances::deposit_creating(&i, 1_000_000);
			let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(i), candidates[0].who, amount);
			System::set_block_number(0);
			XodeStaking::on_initialize(System::block_number());
			
			let desired_candidates = DesiredCandidates::<Test>::get();
			assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");
	
			for i in 0..3 {
				let _ = Balances::deposit_creating(&desired_candidates[i], 1_000_000);
				let k = i as u8 + 1u8;
				let key = sr25519::Public::from_raw([k; 32]);
				let session_keys = SessionKeys { aura: key.into(),};
				let result = Session::set_keys(
					RuntimeOrigin::signed(desired_candidates[i]), 
					session_keys.clone(), 
					Vec::new()
				);
				println!("{:?} free balance: {:?}, {:?}: {:?}",desired_candidates[i], 
					Balances::free_balance(&desired_candidates[i]), 
					session_keys, 
					result
				);
				assert!(result.is_ok(), "Failed to set session keys: {:?}", result);
			}
	
			println!("Delegate: {:?}-{:?}",&i,Balances::free_balance(&i));
			assert_eq!(Balances::free_balance(&i), 1_000_000 - amount);
		}

		for i in 21..24 {
			let amount = (i as u128 - 20) * 1_000;
			let _ = Balances::deposit_creating(&i, 1_000_000);
			let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(i), candidates[1].who, amount);

			println!("Delegate: {:?}-{:?}",&i,Balances::free_balance(&i));
			assert_eq!(Balances::free_balance(&i), 1_000_000 - amount);
		}		

		set_new_block_with_author(&mut candidates[1], System::block_number());

		candidates[0].total_stake = 6_000;
		candidates[0].last_updated = System::block_number();
		candidates[1].total_stake = 6_000;
		candidates[1].last_updated = System::block_number();

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[0]);
		assert_eq!(proposed_candidates[1], candidates[1]);	

		set_new_block_with_author(&mut candidates[0], System::block_number() + 1);

		// Unstake the 3rd delegate in candidate[0]
		let _ = XodeStaking::unstake_candidate(RuntimeOrigin::signed(13), 1);
		
		// 6_000 - 3_000 = 3_000
		candidates[0].total_stake = 3_000;
		candidates[0].last_updated = System::block_number();

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[1]);
		assert_eq!(proposed_candidates[1], candidates[0]);

		set_new_block_with_author(&mut candidates[1], 4 * MINUTES);

		XodeStaking::new_session(4);		
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			5, 
			5, 
			5, 
			2, 
			5,
			4,
			1,
		);

		candidates[0].status = Status::Authoring;
		candidates[0].status_level = 1;
		candidates[1].status = Status::Authoring;
		candidates[1].status_level = 1;
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[1], "Must match");
		assert_eq!(proposed_candidates[1], candidates[0], "Must match");
		
		// =======================================================================
		// SCENE 5 (Offline): Within Session 4, 5 and Session 6 initialization
		// ----------------------------------------------------------------------- 
		// 1. Make one proposed candidate go offline
		// 2. Downgraded after set to offline after two sessions.
		// 3. Todo: Test to online if the status is not yet downgraded
		// 4. Todo: If the candidate is offline or in the process of offline. (Separate
		//          Test).
		//          test_pallet_xode_staking_offline()
		//		4.1. Cannot bond
		//		4.2. Cannot stake
		//		4.3. Can un-stake
		// =======================================================================
		set_new_block_with_author(&mut candidates[0], System::block_number() + 1);

		let _ = XodeStaking::offline_candidate(RuntimeOrigin::signed(candidates[1].who));

		candidates[1].offline = true;
		candidates[1].last_updated = System::block_number();

		// Since we offline, the sort is now reverted back
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[0]);
		assert_eq!(proposed_candidates[1], candidates[1]);		

		set_new_block_with_author(&mut candidates[1], 5 * MINUTES);

		XodeStaking::new_session(5);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			5, 
			5, 
			5, 
			2, 
			4,
			5,
			1,
		);

		set_new_block_with_author(&mut candidates[0], 6 * MINUTES);

		XodeStaking::new_session(6);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			5, 
			4, 
			4, 
			2, 
			4,
			5,
			2,
		);

		candidates[1].status = Status::Queuing;
		candidates[1].status_level = 0;

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[0]);
		assert_eq!(proposed_candidates[1], candidates[1]);
		
		// =======================================================================
		// SCENE 6 (Online): Within Session 6, 7, 8 and Session 9 initialization 
		// ----------------------------------------------------------------------- 
		// 1. Make one proposed candidate go online after being offline
		// 2. Todo: Control, set online only if the status is already queuing
		// =======================================================================
		set_new_block_with_author(&mut candidates[1], System::block_number() + 1);

		let _ = XodeStaking::online_candidate(RuntimeOrigin::signed(candidates[1].who));

		candidates[1].offline = false;
		candidates[1].last_updated = System::block_number();

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[1]);
		assert_eq!(proposed_candidates[1], candidates[0]);	

		set_new_block_with_author(&mut candidates[0], 7 * MINUTES);

		XodeStaking::new_session(7);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			4, 
			4, 
			4, 
			2, 
			5,
			6,
			1,
		);

		candidates[1].status = Status::Queuing;

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[1]);
		assert_eq!(proposed_candidates[1], candidates[0]);

		set_new_block_with_author(&mut candidates[1], 8 * MINUTES);

		XodeStaking::new_session(8);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			4, 
			5, 
			5, 
			2, 
			5,
			6,
			2,
		);

		candidates[1].status = Status::Authoring;

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[1]);
		assert_eq!(proposed_candidates[1], candidates[0]);

		set_new_block_with_author(&mut candidates[0], 9 * MINUTES);

		XodeStaking::new_session(9);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			5, 
			5, 
			5, 
			2, 
			5,
			6,
			3,
		);

		candidates[1].status_level = 1;

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[1]);
		assert_eq!(proposed_candidates[1], candidates[0]);

		// =======================================================================
		// SCENE 7 (Leaving): Within Session 9, 10, 11, 12, 13 and Session 14 initialization
		// ----------------------------------------------------------------------- 
		// 1. Add a new candidate (Candidate-3), bond then leave
		// 2. Make sure to first offline the proposed candidate before leaving
		// 3. Todo: Control, if on the process of leaving: (Separate Test)
		//          test_pallet_xode_staking_leaving()
		//		3.1. Cannot online/offline
		//		3.2. Cannot register
		//		3.3. Cannot bond
		//		3.4. Cannot stake
		//		3.5. Can still un-stake
		// 4. Todo: Once the candidate has been removed in the authorities: (Separate
		//		    Test)
		//          test_pallet_xode_staking_left()
		//		4.1. Unreserve the bond
		//		4.2. Unreserve the stakes
		//		4.3. Remove all the delegation
		//		4.4. Remove the proposed candidate
		//		4.5. Test adding the same candidate again
		// =======================================================================
		set_new_block_with_author(&mut candidates[1], System::block_number() + 1);

		candidates[2] = CandidateInfo {
			who: 3,
			bond: 0,
			total_stake: 0,
			last_updated: System::block_number(),
			last_authored: System::block_number(),
			leaving: false,
			offline: false,
			commission: 0,
			status: Status::Online,
			status_level: 0,
		};

		let _ = XodeStaking::register_candidate(RuntimeOrigin::signed(candidates[2].who));

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates.len(), 3, "Increase by 1");
		assert_eq!(proposed_candidates[2], candidates[2], "Must match");		

		let _ = Balances::deposit_creating(&candidates[2].who, 1_000_000);
		let key = sr25519::Public::from_raw([13u8; 32]);
		let session_keys = SessionKeys { aura: key.into(),};
		let result = Session::set_keys(RuntimeOrigin::signed(candidates[2].who), session_keys.clone(), Vec::new());
		assert!(result.is_ok(), "Failed to set session keys: {:?}", result);

		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(3), 200);
		assert_eq!(Balances::free_balance(&candidates[2].who), 999_800);

		candidates[2].bond = 200;
		candidates[2].last_updated = System::block_number();

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[2], candidates[2], "Must match");

		set_new_block_with_author(&mut candidates[0], 10 * MINUTES);

		XodeStaking::new_session(10);	
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			5, 
			5, 
			5, 
			3, 
			6,
			7,
			1,
		);

		candidates[2].status = Status::Waiting;

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[2], candidates[2], "Still waiting, do not leave");

		set_new_block_with_author(&mut candidates[0], 11 * MINUTES);

		XodeStaking::new_session(11);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			5, 
			6, 
			6, 
			3, 
			6,
			7,
			2,
		);	

		// No queuing status, if new waiting candidate.  Immediately goes to authoring
		// level 0.
		candidates[2].status = Status::Authoring;

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[2], candidates[2], "Status is now authoring level 0 (no queuing).");

		set_new_block_with_author(&mut candidates[1], System::block_number() + 1);

		// Now set to offline before leaving
		let _ = XodeStaking::offline_candidate(RuntimeOrigin::signed(candidates[2].who));
		candidates[2].offline = true;
		candidates[2].last_updated = System::block_number();

		set_new_block_with_author(&mut candidates[0], 12 * MINUTES);

		XodeStaking::new_session(12);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			6, 
			6, 
			6, 
			3, 
			5,
			7,
			3,
		);	

		set_new_block_with_author(&mut candidates[1], 13 * MINUTES);

		XodeStaking::new_session(12);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			6, 
			5, 
			5, 
			3, 
			5,
			7,
			4,
		);

		candidates[2].status = Status::Queuing;
		candidates[2].last_updated = System::block_number(); // Todo: Why?

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[2], candidates[2], "Status is now queuing after being offline.");

		set_new_block_with_author(&mut candidates[1], 14 * MINUTES);

		XodeStaking::new_session(14);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			5, 
			5, 
			5, 
			3, 
			5,
			7,
			5,
		);

		candidates[2].status = Status::Waiting;
		candidates[2].last_updated = System::block_number(); // Todo: Why?

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[2], candidates[2], "Status is now waiting (Ready to leave)");

		set_new_block_with_author(&mut candidates[0], System::block_number() + 1);

		// Now leave
		let leaving = XodeStaking::leave_candidate(RuntimeOrigin::signed(candidates[2].who));
		println!("Leaving {:?}",leaving);

		candidates[2].leaving = true;
		candidates[2].last_updated = System::block_number();

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates.len(), 3, "The number of proposed candidates is still 3");
		assert_eq!(proposed_candidates[2], candidates[2], "Must match with the last proposed candidate, because it is leaving.");	

		set_new_block_with_author(&mut candidates[1], 15 * MINUTES);

		XodeStaking::new_session(15);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			5, 
			5, 
			5, 
			2, // Candidate[2] has left the building
			5,
			7,
			6,
		);
	});
}

#[test]
fn test_pallet_xode_staking_fees_treasury_author_share() {
	test1_ext().execute_with(|| {
		System::set_block_number(0);
		System::on_initialize(0);
		XodeStaking::on_initialize(System::block_number());

		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");

		let _ = Balances::deposit_creating(&desired_candidates[0], 123_456_789_012_345);
		let _ = Balances::deposit_creating(&1, 10_000_000_000);

		System::set_block_number(1);
		System::on_initialize(1);
		XodeStaking::on_initialize(1);

		AuthorGiven::set_author(1);
		Authorship::on_initialize(1);
		Authorship::on_finalize(1);

		let author = Authorship::author();
		assert_eq!(author, Some(1));

		println!("Before Balance: {:?}",Balances::free_balance(desired_candidates[0].clone()));

		// Construct the call
		let call = RuntimeCall::XodeStaking(crate::Call::register_candidate{});
		let info = call.get_dispatch_info();
		let len = call.encode().len();
		// Weight { ref_time: 139_300_000, proof_size: 8_587 }
		println!("Info: {:?}",info.clone());

		// Dispatch the call
		let _ = ChargeTransactionPayment::<Test>::from(0).pre_dispatch(
			&desired_candidates[0], 
			&call.clone(), 
			&info, 
			len
		).expect("pre_dispatch error");
		let post_result = call.clone().dispatch(RuntimeOrigin::signed(desired_candidates[0].clone())).expect("dispatch failure");
		let actual_fee = TransactionPayment::compute_actual_fee(len.try_into().unwrap(), &info, &post_result, 0);
		// 123_456_789_012_345 - 22_114_400 = 123_456_766_897_945
		println!("After Balance: {:?}",Balances::free_balance(desired_candidates[0].clone()));
		println!("Fee: {:?}",actual_fee);
		
		//Typical withdraw
		//let imbalance = Balances::withdraw(&desired_candidates[0], actual_fee, WithdrawReasons::TRANSACTION_PAYMENT, ExistenceRequirement::KeepAlive).expect("Fee withdrawal should succeed");
		//println!("Imbalance: {:?}",imbalance);

		// Withdraw with DealWithFees implementation on charge transaction
		type FungibleAdapterT = FungibleAdapter<Balances, DealWithFees<Test>>;
		let imbalance = <FungibleAdapterT as OnChargeTransaction<Test>>::withdraw_fee(
			&desired_candidates[0],
			&call.clone(), 
			&info,
			actual_fee,
			0
		).expect("pre_dispatch error");
		println!("Imbalance: {:?}",imbalance);

		// Deal with fees
		DealWithFees::<Test>::on_unbalanceds(vec![imbalance.unwrap()].into_iter());

		// 22_114_400 * 20% = 4_422_880
		assert_eq!(Balances::free_balance(XodeTreasuryAccount::get()), 4_422_880);
		// 22_114_400 * 80% = 17_691_520
		// 10_000_000_000 + 17_691_520 = 10_017_691_520
		assert_eq!(Balances::free_balance(1), 10_017_691_520);

	});
}

#[test]
fn test_pallet_xode_staking_fees_author_delegator_share()  {
	test1_ext().execute_with(|| {
		System::set_block_number(0);
		System::on_initialize(0);
		XodeStaking::on_initialize(System::block_number());

		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");

		// Candidate (Author)
		let _ = Balances::deposit_creating(&1, 100_000_000_000_000); // Author
		let _ = Balances::deposit_creating(&2, 100_000_000_000_000); // Caller
		// Delegator (5 accounts)
		let _ = Balances::deposit_creating(&11, 100_000_000_000_000);
		let _ = Balances::deposit_creating(&12, 100_000_000_000_000);
		let _ = Balances::deposit_creating(&13, 100_000_000_000_000);
		let _ = Balances::deposit_creating(&14, 100_000_000_000_000);
		let _ = Balances::deposit_creating(&15, 100_000_000_000_000);

		System::set_block_number(1);
		System::on_initialize(1);
		XodeStaking::on_initialize(1);

		// Set the author
		AuthorGiven::set_author(1);
		Authorship::on_initialize(1);
		Authorship::on_finalize(1);

		let author = Authorship::author();
		assert_eq!(author, Some(1));

		// Register the candidate and put session keys
		let _ = XodeStaking::register_candidate(RuntimeOrigin::signed(1));
		let key = sr25519::Public::from_raw([1u8; 32]);
		let session_keys = SessionKeys { aura: key.into(),};
		let result = Session::set_keys(RuntimeOrigin::signed(1), session_keys.clone(), Vec::new());
		assert!(result.is_ok(), "Failed to set session keys: {:?}", result);

		// Bond candidate and set commission
		// 100_000_000_000_000 - 10_000_000_000_000 = 90_000_000_000_000
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(1), 10_000_000_000_000);
		assert_eq!(Balances::free_balance(&1), 90_000_000_000_000);

		// Set the commission to 50%
		let _ = XodeStaking::set_commission_of_candidate(RuntimeOrigin::signed(1), 50);

		// Stake
		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(11), 1, 10_000_000_000_000);
		assert_eq!(Balances::free_balance(&11), 90_000_000_000_000);
		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(12), 1, 20_000_000_000_000);
		assert_eq!(Balances::free_balance(&12), 80_000_000_000_000);
		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(13), 1, 30_000_000_000_000);
		assert_eq!(Balances::free_balance(&13), 70_000_000_000_000);
		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(14), 1, 40_000_000_000_000);
		assert_eq!(Balances::free_balance(&14), 60_000_000_000_000);
		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(15), 1, 50_000_000_000_000);
		assert_eq!(Balances::free_balance(&15), 50_000_000_000_000);

		// Construct the call (register a candidate, e.g., 2)
		println!("Before dispatch: {:?}",Balances::free_balance(2));
		let call = RuntimeCall::XodeStaking(crate::Call::register_candidate { });
		let info = call.get_dispatch_info();
		let len = call.encode().len();
		println!("Info: {:?}",info.clone());

		// Dispatch the call
		let _ = ChargeTransactionPayment::<Test>::from(0).pre_dispatch(
			&2, 
			&call.clone(), 
			&info, 
			len
		).expect("pre_dispatch error");
		let post_result = call.clone().dispatch(RuntimeOrigin::signed(2)).expect("dispatch failure");
		let actual_fee = TransactionPayment::compute_actual_fee(len.try_into().unwrap(), &info, &post_result, 0);
		// actual fee: 22_114_400
		println!("Fee: {:?}",actual_fee);
		
		// 100_000_000_000_000 - 22_114_400 = 99_999_977_885_600
		println!("After dispatch: {:?}",Balances::free_balance(2));
		assert_eq!(Balances::free_balance(&2), 99_999_977_885_600);

		// Withdraw with DealWithFees implementation on charge transaction
		type FungibleAdapterT = FungibleAdapter<Balances, DealWithFees<Test>>;
		let imbalance = <FungibleAdapterT as OnChargeTransaction<Test>>::withdraw_fee(
			&2,
			&call.clone(), 
			&info,
			actual_fee,
			0
		).expect("pre_dispatch error");
		println!("Imbalance: {:?}",imbalance);

		// Deal with fees
		DealWithFees::<Test>::on_unbalanceds(vec![imbalance.unwrap()].into_iter());

		// 22_114_400 * 20% = 4_422_880
		assert_eq!(Balances::free_balance(XodeTreasuryAccount::get()), 4_422_880);

		// Starting: 22_114_400 * 80% = 17_691_520
		// Commission = 50%
		
		// Staker 1 Ratio (1/15=6%) = 17_691_520 * 0.5 * 0.06 = 530_746 
		// 90_000_000_000_000 + 530_746 = 90_000_000_530_746 
		assert_eq!(Balances::free_balance(11), 90_000_000_530_746);

		// Remaining = 17_691_520 - 530_746 = 17_160_774
		// Staker 2 Ratio (2/15=13%) = 17_160_774 * 0.5 * 0.13 = 1_115_451 
		// 80_000_000_000_000 + 1_115_451 = 80_000_001_115_451 
		assert_eq!(Balances::free_balance(12), 80_000_001_115_451);

		// Remaining = 17_160_774 - 1_115_451 = 16_045_323
		// Staker 3 Ratio (3/15=20%) = 16_045_323 * 0.5 * 0.20 = 1_604_533 
		// 70_000_000_000_000 + 1_604_533 = 70_000_001_604_533
		assert_eq!(Balances::free_balance(13), 70_000_001_604_533);		

		// Remaining = 16_045_323 - 1_604_533 = 14_440_790
		// Staker 4 Ratio (4/15=26%) = 14_440_790 * 0.5 * 0.26 = 1_877_303 
		// 60_000_000_000_000 + 1_877_303 = 60_000_001_877_303
		assert_eq!(Balances::free_balance(14), 60_000_001_877_303);			

		// Remaining = 14_440_790 - 1_877_303 = 12_563_487
		// Staker 5 Ratio (5/15=33%) = 12_563_487 * 0.5 * 0.33 = 2_072_976 
		// 50_000_000_000_000 + 2_072_976 = 50_000_002_072_976
		assert_eq!(Balances::free_balance(15), 50_000_002_072_976);		

		// Remaining = 12_563_487 - 2_072_976 = 10_490_511
		// 90_000_000_000_000 + 10_490_511 = 90_000_010_490_511
		assert_eq!(Balances::free_balance(1), 90_000_010_490_511);
	});
}

#[test]
fn test_pallet_xode_staking_author_staling() {
	test1_ext().execute_with(|| {
		System::set_block_number(0);
		XodeStaking::on_initialize(System::block_number());
		
		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");

		for i in 0..3 {
			let _ = Balances::deposit_creating(&desired_candidates[i], 1_000_000);
			let k = i as u8 + 1u8;
			let key = sr25519::Public::from_raw([k; 32]);
			let session_keys = SessionKeys { aura: key.into(),};
			let result = Session::set_keys(
				RuntimeOrigin::signed(desired_candidates[i]), 
				session_keys.clone(), 
				Vec::new()
			);
			println!("{:?} free balance: {:?}, {:?}: {:?}",desired_candidates[i], 
				Balances::free_balance(&desired_candidates[i]), 
				session_keys, 
				result
			);
			assert!(result.is_ok(), "Failed to set session keys: {:?}", result);
        }

		System::set_block_number((1 * MINUTES).into());
		XodeStaking::on_initialize(System::block_number());

		check_staking_storages(
			0, 
			0, 
			0, 
			0, 
			3,
			8,
			1,
		);
		
		let mut candidates: [CandidateInfo<AccountId, Balance, BlockNumber>; 3] = [
			Default::default(), 
			Default::default(),
			Default::default(),
		];
		for i in 0..3 {
			let account_id = i as u64 + 1;
			let candidate = CandidateInfo {
				who: account_id,
				bond: 0,
				total_stake: 0,
				last_updated: System::block_number(),
				last_authored: System::block_number(),
				leaving: false,
				offline: false,
				commission: 0,
				status: Status::Online,
				status_level: 0,
			};
			candidates[i] = candidate;
			let _ = XodeStaking::register_candidate(RuntimeOrigin::signed(account_id));
		}
	
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates.len(), 3, "The number of proposed candidates should be 3");
	
		for i in 0..3 {
			assert_eq!(proposed_candidates[i], candidates[i], "Must match");
	
			let _ = Balances::deposit_creating(&proposed_candidates[i].who, 1_000_000);
			let k = i as u8 + 10u8;
			let key = sr25519::Public::from_raw([k; 32]);
			let session_keys = SessionKeys { aura: key.into(),};
			let result = Session::set_keys(
				RuntimeOrigin::signed(proposed_candidates[i].who), 
				session_keys.clone(), 
				Vec::new()
			);
			assert!(result.is_ok(), "Failed to set session keys: {:?}", result);
	
			let b = (i as u128 + 1u128) * 100u128;
			let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(proposed_candidates[i].who), b);
			assert_eq!(Balances::free_balance(&proposed_candidates[i].who), 1_000_000u128 - b);
	
			candidates[i].bond = b;
			candidates[i].last_updated = System::block_number();
		}
	
		// Index reverse because of sorting based on bond
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[2], "Must match");
		assert_eq!(proposed_candidates[1], candidates[1], "Must match");
		assert_eq!(proposed_candidates[2], candidates[0], "Must match");

		set_new_block_with_author(&mut candidates[0], 2 * MINUTES);

		XodeStaking::new_session(2);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			0, 
			3, 
			3, 
			3, 
			6,
			8,
			2,
		);

		set_new_block_with_author(&mut candidates[1], 3 * MINUTES);

		XodeStaking::new_session(3);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			3, 
			6, 
			6, 
			3, 
			6,
			8,
			3,
		);

		set_new_block_with_author(&mut candidates[0], 4 * MINUTES);

		XodeStaking::new_session(4);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			6, 
			6, 
			6, 
			3, 
			6,		
			8,
			4,
		);

		set_new_block_with_author(&mut candidates[0], 5 * MINUTES);

		XodeStaking::new_session(5);
		Session::on_initialize(System::block_number()); 

		check_staking_storages(
			6, 
			6, 
			6, 
			3, 
			5,	// candidates[2] is staling
			8,
			5,
		);

		candidates[2].offline = true;
		candidates[2].last_updated = System::block_number();
		candidates[2].status = Status::Authoring;

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidates[2], "Must match");
	});
}

#[test]
fn test_pallet_xode_staking_unstaked() {
	test1_ext().execute_with(|| {
		System::set_block_number(0);

		XodeStaking::on_initialize(System::block_number());
		
		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");
	});
}

#[test]
fn test_pallet_xode_staking_offline() {
	test1_ext().execute_with(|| {
		System::set_block_number(0);

		XodeStaking::on_initialize(System::block_number());
		
		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");
	});
}

#[test]
fn test_pallet_xode_staking_leaving() {
	test1_ext().execute_with(|| {
		System::set_block_number(0);

		XodeStaking::on_initialize(System::block_number());
		
		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");
	});
}

#[test]
fn test_pallet_xode_staking_left() {
	test1_ext().execute_with(|| {
		System::set_block_number(0);

		XodeStaking::on_initialize(System::block_number());
		
		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");
	});
}