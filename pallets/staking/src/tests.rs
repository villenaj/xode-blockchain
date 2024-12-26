use crate::{mock::*, 
	DesiredCandidates, ProposedCandidates, CandidateInfo,
	WaitingCandidates,
};
use pallet_collator_selection::Invulnerables;
use frame_support::traits::Hooks;
use pallet_session::SessionManager;
use frame_support::traits::Currency;

#[test]
fn test_pallet_xode_staking() {
	test1_ext().execute_with(|| {
		// Scenario 1: Check the desired candidates from runtime config
		System::set_block_number(1);
		XodeStaking::on_initialize(1);
		let desired_candidates = DesiredCandidates::<Test>::get();
		assert_eq!(desired_candidates.len(), 3, "There should be exactly three desired candidates");
		
		// Scenario 2: Check the invulnerabilities (current authors)
		XodeStaking::new_session(1);
		assert_eq!(Invulnerables::<Test>::get(), desired_candidates, "The invulnerables should match the desired candidates");

		// Scenario 3: Add candidates (at least 2 candidates) [Candidate-1, Candidate-2]
		let _ = XodeStaking::register_candidate(RuntimeOrigin::signed(1));
		let _ = XodeStaking::register_candidate(RuntimeOrigin::signed(2));
		let mut candidate_1 = CandidateInfo {
			who: 1,
			bond: 0,
			total_stake: 0,
			last_updated: 1,
			leaving: false,
			offline: false,
			commission: 0,
		};
		let mut candidate_2 = CandidateInfo {
			who: 2,
			bond: 0,
			total_stake: 0,
			last_updated: 1,
			leaving: false,
			offline: false,
			commission: 0,
		};
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates.len(), 2, "The number of proposed candidates should be 2");
		assert_eq!(proposed_candidates[0], candidate_1, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_2, "The second candidate data does not match");

		// Scenario 4: Bond the candidate
		// Note:
		//      1. Sorted proposed candidates
		//      2. Last updated block number
		System::set_block_number(2);
		let _ = Balances::deposit_creating(&1, 1000);
		let _ = Balances::deposit_creating(&2, 1000);

		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(1), 100);
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(2), 200);

		candidate_1.bond = 100;
		candidate_1.last_updated = 2;
		candidate_2.bond = 200;
		candidate_2.last_updated = 2;
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_2, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_1, "The second candidate data does not match");

		// Scenario 5: Increase the bond of a candidate
		// Note:
		//      1. Sorted proposed candidates
		//      2. Check if the bond amount is correct
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(1), 400);

		candidate_1.bond = 400;
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_1, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_2, "The second candidate data does not match");

		// Scenario 6: Decrease the bond of a candidate
		// Note:
		//      1. Sorted proposed candidates (using last updated since bond amount is equal)
		//      2. Check if the bond amount is correct
		System::set_block_number(3);
		let _ = XodeStaking::bond_candidate(RuntimeOrigin::signed(1), 200);

		candidate_1.bond = 200;
		candidate_1.last_updated = 3;
		let proposed_candidates = ProposedCandidates::<Test>::get();
		assert_eq!(proposed_candidates[0], candidate_2, "The first candidate data does not match");
		assert_eq!(proposed_candidates[1], candidate_1, "The second candidate data does not match");

		// Scenario 7: Move to the next session
		// Note:
		//		1. Move two (2) sessions so that the waiting candidates are added to the invulnerable
		System::set_block_number(4);
		XodeStaking::new_session(2);
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

		System::set_block_number(5);
		XodeStaking::new_session(3);

		let invulnerables = Invulnerables::<Test>::get();
		assert_eq!(invulnerables.len(), 5, "There should be exactly now 5");	

		assert_eq!(invulnerables[0], xaver1);
		assert_eq!(invulnerables[1], xaver2);
		assert_eq!(invulnerables[2], xaver3);
		assert_eq!(invulnerables[3], candidate_2.who);
		assert_eq!(invulnerables[4], candidate_1.who);

		// Todo: Add stakes (at least 3 stakes per candidate)
		// Note:
		//		1. Check the proposed candidate values
		let _ = Balances::deposit_creating(&11, 1000);
		let _ = Balances::deposit_creating(&12, 1000);
		let _ = Balances::deposit_creating(&13, 1000);
		let _ = Balances::deposit_creating(&21, 1000);
		let _ = Balances::deposit_creating(&22, 1000);
		let _ = Balances::deposit_creating(&23, 1000);

		assert_eq!(Balances::free_balance(&11), 1000);

		//let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(11), 1, 10);
		//let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(12), 1, 20);
		//let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(13), 1, 30);

		//let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(21), 2, 10);
		//let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(22), 2, 20);
		//let _ = XodeStaking::stake_candidate(RuntimeOrigin::signed(23), 2, 30);

		//candidate_1.total_stake = 60;
		//candidate_1.last_updated = 5;
		//candidate_2.total_stake = 60;
		//candidate_2.last_updated = 5;

		//let proposed_candidates = ProposedCandidates::<Test>::get();
		//assert_eq!(proposed_candidates[0], candidate_1);
		//assert_eq!(proposed_candidates[1], candidate_2);


		// Todo: Un-stake 1 delegator per candidate

		// Todo: Make one candidate to go offline [Candidate-B]

		// Todo: Make the offline candidate to go online [Candidate-B]

		// Todo: Add another candidate (do not bond) [Candidate-D]

		// Todo: Add another candidate then bond [Candidate-E]

		// Todo: Stake the new candidate using the delegator of the first candidate [Candidate-E, Candidate-A(first candidate)]

		// Todo: Set the first candidate to leaving [Candidate-A]

		// Todo: Now that only Candidate-B and Candidate-E are online, let them author a block

		// Todo: Candidate-B did not author a block
	});
}

