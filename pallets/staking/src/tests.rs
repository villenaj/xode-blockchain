use crate::{mock::*, CandidateInfo, 
	DesiredCandidates, ProposedCandidates, WaitingCandidates,
};
use pallet_collator_selection::Invulnerables;
use frame_support::traits::Hooks;
use pallet_session::SessionManager;
use frame_support::traits::Currency;
use sp_core::sr25519;

#[test]
fn test_pallet_xode_staking() {
	test1_ext().execute_with(|| {
		// Scenario 1 (SESSION 1): Check the desired candidates from runtime config
		// Note:
		//		1. Must provide balances before setting keys
		// ========================================================================
		System::set_block_number(0);
		XodeStaking::on_initialize(System::block_number());
		
		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");

		let _ = Balances::deposit_creating(&desired_candidates[0], 1000);
		let key = sr25519::Public::from_raw([1u8; 32]);
		let session_keys = SessionKeys { aura: key.into(),};
		let result = Session::set_keys(RuntimeOrigin::signed(desired_candidates[0]), session_keys.clone(), Vec::new());
		println!("{:?} free balance: {:?}, {:?}: {:?}",desired_candidates[0], Balances::free_balance(&desired_candidates[0]), session_keys, result);
		assert!(result.is_ok(), "Failed to set session keys: {:?}", result);
		let _ = Balances::deposit_creating(&desired_candidates[1], 1000);
		let key = sr25519::Public::from_raw([2u8; 32]);
		let session_keys = SessionKeys { aura: key.into(),};
		let result = Session::set_keys(RuntimeOrigin::signed(desired_candidates[1]), session_keys.clone(), Vec::new());
		println!("{:?} free balance: {:?}, {:?}: {:?}",desired_candidates[1], Balances::free_balance(&desired_candidates[1]), session_keys, result);
		assert!(result.is_ok(), "Failed to set session keys: {:?}", result);
		let _ = Balances::deposit_creating(&desired_candidates[2], 1000);
		let key = sr25519::Public::from_raw([3u8; 32]);
		let session_keys = SessionKeys { aura: key.into(),};
		let result = Session::set_keys(RuntimeOrigin::signed(desired_candidates[2]), session_keys.clone(), Vec::new());
		println!("{:?} free balance: {:?}, {:?}: {:?}",desired_candidates[2], Balances::free_balance(&desired_candidates[2]), session_keys, result);
		assert!(result.is_ok(), "Failed to set session keys: {:?}", result);

		System::set_block_number((1 * MINUTES).into());
		XodeStaking::on_initialize(System::block_number());
		XodeStaking::new_session(1);
		
		let invulnerables = pallet_collator_selection::Invulnerables::<Test>::get();
		println!("Invulnerables {:?}",invulnerables);
		assert_eq!(invulnerables.len(), 3, "Invulerables after new session must have 3 entries, equal to desired candidates");

		// Scenario 2: Check the invulnerabilities (current authors)
		// Note:
		//		1. Must call session on initialize hook to consume the queued keys of the
		//		   previous session
		// =======================================================================
		System::set_block_number((2 * MINUTES).into());
		XodeStaking::on_initialize(System::block_number());
		XodeStaking::new_session(2);

		pallet_session::Pallet::<Test>::on_initialize(System::block_number()); 

		let invulnerables = pallet_collator_selection::Invulnerables::<Test>::get();
		println!("Invulnerables {:?}",invulnerables);
		//assert_eq!(invulnerables.len(), 3, "Invulerables after new session must have 3 entries, equal to desired candidates");

		let authorities = pallet_aura::Authorities::<Test>::get();
		println!("Authorities {:?}",authorities);
		//assert_eq!(authorities.len(), 3, "Authorities are exactly equal to invulnerables.");

		let queued_keys = pallet_session::QueuedKeys::<Test>::get();
		println!("Keys {:?}",queued_keys);
		assert_eq!(queued_keys.len(), 3, "Keys are exactly equal to invulnerables.");


		// Scenario 3: Add candidates (at least 2 candidates) [Candidate-1, Candidate-2]
		// Note:
		//		1. Added a balance of the two registered candidates
		//		2. Added also keys for the registered candidates
		// =======================================================================
		let _ = XodeStaking::register_candidate(RuntimeOrigin::signed(1));
		let _ = XodeStaking::register_candidate(RuntimeOrigin::signed(2));
		let mut candidate_1 = CandidateInfo {
			who: 1,
			bond: 0,
			total_stake: 0,
			last_updated: System::block_number(),
			leaving: false,
			offline: false,
			commission: 0,
		};
		let mut candidate_2 = CandidateInfo {
			who: 2,
			bond: 0,
			total_stake: 0,
			last_updated: System::block_number(),
			leaving: false,
			offline: false,
			commission: 0,
		};
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates.len(), 2, "The number of proposed candidates should be 2");
		assert_eq!(proposed_candidates[0], candidate_1, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_2, "The second candidate data does not match");

		let _ = Balances::deposit_creating(&1, 1000);
		let key = sr25519::Public::from_raw([11u8; 32]);
		let session_keys = SessionKeys { aura: key.into(),};
		let result = Session::set_keys(RuntimeOrigin::signed(1), session_keys.clone(), Vec::new());
		assert!(result.is_ok(), "Failed to set session keys: {:?}", result);
		let _ = Balances::deposit_creating(&2, 1000);
		let key = sr25519::Public::from_raw([12u8; 32]);
		let session_keys = SessionKeys { aura: key.into(),};
		let result = Session::set_keys(RuntimeOrigin::signed(2), session_keys.clone(), Vec::new());
		assert!(result.is_ok(), "Failed to set session keys: {:?}", result);

		// Scenario 4: Bond the candidate
		// Note:
		//      1. Sorted proposed candidates
		//      2. Last updated block number
		// =======================================================================
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(1), 100);
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(2), 200);

		assert_eq!(Balances::free_balance(&1), 900);
		assert_eq!(Balances::free_balance(&2), 800);

		candidate_1.bond = 100;
		candidate_1.last_updated = System::block_number();
		candidate_2.bond = 200;
		candidate_2.last_updated = System::block_number();
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_2, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_1, "The second candidate data does not match");

		// Scenario 5: Increase the bond of a candidate
		// Note:
		//      1. Sorted proposed candidates
		//      2. Check if the bond amount is correct
		// =======================================================================
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(1), 400);

		assert_eq!(Balances::free_balance(&1), 600);

		candidate_1.bond = 400;
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_1, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_2, "The second candidate data does not match");

		// Scenario 6: Decrease the bond of a candidate
		// Note:
		//      1. Sorted proposed candidates (using last updated since bond amount is equal)
		//      2. Check if the bond amount is correct
		// =======================================================================
		System::set_block_number(System::block_number() + 1);
		XodeStaking::on_initialize(System::block_number() + 1);

		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(1), 200);

		assert_eq!(Balances::free_balance(&1), 800);

		candidate_1.bond = 200;
		candidate_1.last_updated = System::block_number();
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_2, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_1, "The second candidate data does not match");

		// Scenario 7 (SESSION 3-4): Move to the next session
		// Note:
		//		1. Move two (2) sessions so that the waiting candidates are added to the invulnerable
		// =======================================================================
		System::set_block_number((3 * MINUTES).into());
		XodeStaking::on_initialize(System::block_number());
		XodeStaking::new_session(3);

		let waiting_candidates = WaitingCandidates::<Test>::get();
		assert_eq!(waiting_candidates.len(), 5, "There should be exactly five waiting candidates");

		let xaver1 = 13620103657161844528u64;
		let xaver2 = 14516343343327982992u64;
		let xaver3 = 10654826648675244518u64;
		
		assert_eq!(waiting_candidates[0], xaver1);
		assert_eq!(waiting_candidates[1], xaver2);
		assert_eq!(waiting_candidates[2], xaver3);
		assert_eq!(waiting_candidates[3], candidate_2.who);
		assert_eq!(waiting_candidates[4], candidate_1.who);

		let invulnerables = Invulnerables::<Test>::get();
		assert_eq!(invulnerables.len(), 3, "There should be exactly still 3");

		assert_eq!(invulnerables[0], xaver1);
		assert_eq!(invulnerables[1], xaver2);
		assert_eq!(invulnerables[2], xaver3);

		System::set_block_number((4 * MINUTES).into());
		XodeStaking::on_initialize(System::block_number());
		XodeStaking::new_session(4);

		let invulnerables = Invulnerables::<Test>::get();
		assert_eq!(invulnerables.len(), 5, "There should be exactly now 5");	

		assert_eq!(invulnerables[0], xaver1);
		assert_eq!(invulnerables[1], xaver2);
		assert_eq!(invulnerables[2], xaver3);
		assert_eq!(invulnerables[3], candidate_2.who);
		assert_eq!(invulnerables[4], candidate_1.who);

		let waiting_candidates = WaitingCandidates::<Test>::get();
		assert_eq!(waiting_candidates.len(), 5, "There should be exactly now 5");

		// Scenario 8: Add stakes (at least 3 stakes per candidate)
		// Note:
		//		1. Check the proposed candidate values
		// =======================================================================
		let _ = Balances::deposit_creating(&11, 1000);
		let _ = Balances::deposit_creating(&12, 1000);
		let _ = Balances::deposit_creating(&13, 1000);
		let _ = Balances::deposit_creating(&21, 1000);
		let _ = Balances::deposit_creating(&22, 1000);
		let _ = Balances::deposit_creating(&23, 1000);

		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(11), 1, 10);
		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(12), 1, 20);
		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(13), 1, 30);

		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(21), 2, 10);
		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(22), 2, 20);
		let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(23), 2, 30);

		assert_eq!(Balances::free_balance(&11), 990);
		assert_eq!(Balances::free_balance(&12), 980);
		assert_eq!(Balances::free_balance(&13), 970);

		candidate_1.total_stake = 60;
		candidate_1.last_updated = System::block_number();
		candidate_2.total_stake = 60;
		candidate_2.last_updated = System::block_number();

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_1);
		assert_eq!(proposed_candidates[1], candidate_2);	

		// Scenario 9: Un-stake 1 delegator per candidate
		// Note:
		//		1. Checked the proposed candidate info and sort
		System::set_block_number(6);
		let _ = XodeStaking::unstake_candidate(RuntimeOrigin::signed(13), 1);
		
		candidate_1.total_stake = 30;
		candidate_1.last_updated = 6;

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_2);
		assert_eq!(proposed_candidates[1], candidate_1);

		// Scenario 10 (SESSION 4): Make one candidate to go offline [Candidate-2]
		// Note:
		//		1. Check sort, offline candidates are pushed at the bottom
		//		2. Change the session.  Take note if the candidate did not author a block it will be set offline.
		System::set_block_number(7);

		let _ = XodeStaking::offline_candidate(RuntimeOrigin::signed(2));

		candidate_2.offline = true;
		candidate_2.last_updated = 7;

		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_1);
		assert_eq!(proposed_candidates[1], candidate_2);		


		// XodeStaking::new_session(4);

		//let _ = XodeStaking::prepare_waiting_candidates();
		//let waiting_candidates = WaitingCandidates::<Test>::get();
		//assert_eq!(waiting_candidates.len(), 4, "There should be exactly four (4) waiting candidate");
		
		//assert_eq!(waiting_candidates[0], xaver1);
		//assert_eq!(waiting_candidates[1], xaver2);
		//assert_eq!(waiting_candidates[2], xaver3);
		//assert_eq!(waiting_candidates[3], candidate_1.who);

		// Todo: Make the offline candidate to go online [Candidate-B]


		// Todo: Add another candidate (do not bond) [Candidate-D]

		// Todo: Add another candidate then bond [Candidate-E]

		// Todo: Stake the new candidate using the delegator of the first candidate [Candidate-E, Candidate-A(first candidate)]

		// Todo: Set the first candidate to leaving [Candidate-A]

		// Todo: Now that only Candidate-B and Candidate-E are online, let them author a block

		// Todo: Candidate-B did not author a block
	});
}

