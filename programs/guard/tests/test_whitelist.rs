//! `guard`'s own behavior, tested without `calma` in the picture.
//!
//! The program had no tests of its own — it was only ever exercised as a fixture
//! inside `programs/calma/tests/test_guard.rs`, which asserts that a gated market
//! gates. That leaves the list's own rules (duplicate handling, authority
//! enforcement, and the per-authority PDA split that the module docs warn about
//! at length) covered only incidentally, if at all.

use anchor_lang::{
    solana_program::instruction::Instruction, InstructionData, ToAccountMetas,
};
use guard::state::GuardState;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

use anchor_lang::prelude::Pubkey;

const GUARD_SO: &[u8] = include_bytes!("../../../target/deploy/guard.so");

struct Ctx {
    svm: LiteSVM,
    payer: Keypair,
    authority: Keypair,
    guard_state: Pubkey,
}

fn guard_pda(authority: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"guard", authority.as_ref()], &guard::id()).0
}

/// Build a transaction on a *fresh* blockhash.
///
/// These tests deliberately send the same `check` twice — once expecting failure
/// and once expecting success — and two identical instructions from the same
/// payer on the same blockhash produce the same signature, which the runtime
/// rejects as a duplicate. Expiring first makes each send its own transaction.
fn build(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> VersionedTransaction {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap()
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) -> bool {
    let tx = build(svm, ixs, payer, signers);
    svm.send_transaction(tx).is_ok()
}

/// Send and require success, surfacing the program logs when it fails.
fn must_send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) {
    let tx = build(svm, ixs, payer, signers);
    svm.send_transaction(tx).expect("transaction should succeed");
}

fn setup() -> Ctx {
    let payer = Keypair::new();
    let authority = Keypair::new();
    let mut svm = LiteSVM::new();
    svm.add_program(guard::id(), GUARD_SO).unwrap();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let guard_state = guard_pda(&authority.pubkey());
    let ix = Instruction::new_with_bytes(
        guard::id(),
        &guard::instruction::Create {}.data(),
        guard::accounts::Create {
            guard_state,
            authority: authority.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    must_send(&mut svm, &[ix], &payer, &[&payer, &authority]);

    Ctx {
        svm,
        payer,
        authority,
        guard_state,
    }
}

fn add_ix(guard_state: Pubkey, authority: Pubkey, member: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        guard::id(),
        &guard::instruction::Add { pubkey: member }.data(),
        guard::accounts::Add {
            guard_state,
            authority,
        }
        .to_account_metas(None),
    )
}

fn remove_ix(guard_state: Pubkey, authority: Pubkey, member: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        guard::id(),
        &guard::instruction::Remove { pubkey: member }.data(),
        guard::accounts::Remove {
            guard_state,
            authority,
        }
        .to_account_metas(None),
    )
}

fn check_ix(guard_state: Pubkey, member: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        guard::id(),
        &guard::instruction::Check { pubkey: member }.data(),
        guard::accounts::Check { guard_state }.to_account_metas(None),
    )
}

fn read_state(ctx: &Ctx) -> GuardState {
    let data = ctx.svm.get_account(&ctx.guard_state).unwrap().data;
    GuardState::try_deserialize(&mut data.as_slice()).unwrap()
}

use anchor_lang::AccountDeserialize;

#[test]
fn create_produces_an_empty_list_owned_by_its_authority() {
    let ctx = setup();
    let state = read_state(&ctx);
    assert_eq!(state.authority, ctx.authority.pubkey());
    assert!(state.whitelist.is_empty());
}

#[test]
fn check_fails_until_the_member_is_added_and_again_once_removed() {
    let mut ctx = setup();
    let member = Pubkey::new_unique();
    let (gs, auth) = (ctx.guard_state, ctx.authority.pubkey());
    let payer = ctx.payer.insecure_clone();
    let authority = ctx.authority.insecure_clone();

    assert!(
        !send(&mut ctx.svm, &[check_ix(gs, member)], &payer, &[&payer]),
        "check must fail before the member is whitelisted"
    );

    must_send(
        &mut ctx.svm,
        &[add_ix(gs, auth, member)],
        &payer,
        &[&payer, &authority],
    );
    assert_eq!(read_state(&ctx).whitelist, vec![member]);
    assert!(send(&mut ctx.svm, &[check_ix(gs, member)], &payer, &[&payer]));

    must_send(
        &mut ctx.svm,
        &[remove_ix(gs, auth, member)],
        &payer,
        &[&payer, &authority],
    );
    assert!(read_state(&ctx).whitelist.is_empty());
    assert!(
        !send(&mut ctx.svm, &[check_ix(gs, member)], &payer, &[&payer]),
        "check must fail again once the member is removed"
    );
}

#[test]
fn a_member_cannot_be_added_twice() {
    let mut ctx = setup();
    let member = Pubkey::new_unique();
    let (gs, auth) = (ctx.guard_state, ctx.authority.pubkey());
    let payer = ctx.payer.insecure_clone();
    let authority = ctx.authority.insecure_clone();

    must_send(
        &mut ctx.svm,
        &[add_ix(gs, auth, member)],
        &payer,
        &[&payer, &authority],
    );
    assert!(
        !send(
            &mut ctx.svm,
            &[add_ix(gs, auth, member)],
            &payer,
            &[&payer, &authority]
        ),
        "AlreadyWhitelisted must reject the duplicate"
    );
    assert_eq!(read_state(&ctx).whitelist.len(), 1);
}

#[test]
fn only_the_owning_authority_may_edit_the_list() {
    let mut ctx = setup();
    let intruder = Keypair::new();
    let member = Pubkey::new_unique();
    let gs = ctx.guard_state;
    let payer = ctx.payer.insecure_clone();

    // `has_one = authority` plus the seed derivation means an intruder signing
    // for themselves addresses a different PDA and fails either way.
    assert!(
        !send(
            &mut ctx.svm,
            &[add_ix(gs, intruder.pubkey(), member)],
            &payer,
            &[&payer, &intruder]
        ),
        "a non-authority must not be able to add to someone else's list"
    );
    assert!(read_state(&ctx).whitelist.is_empty());
}

/// The hazard `initialize.rs` and `check.rs` both warn about: creation is
/// permissionless and lists are per-authority, so "owned by the guard program"
/// proves nothing on its own. Anyone can stand up their own list, add
/// themselves, and pass `check` on it — which is why a consumer has to pin the
/// exact `guard_state` address, as `calma` does via `Pool::guard_state`.
#[test]
fn anyone_can_create_their_own_list_and_pass_their_own_check() {
    let mut ctx = setup();
    let attacker = Keypair::new();
    let payer = ctx.payer.insecure_clone();
    let attacker_state = guard_pda(&attacker.pubkey());

    assert_ne!(
        attacker_state, ctx.guard_state,
        "a different authority must get a different PDA"
    );

    let create = Instruction::new_with_bytes(
        guard::id(),
        &guard::instruction::Create {}.data(),
        guard::accounts::Create {
            guard_state: attacker_state,
            authority: attacker.pubkey(),
            payer: payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    );
    must_send(&mut ctx.svm, &[create], &payer, &[&payer, &attacker]);
    must_send(
        &mut ctx.svm,
        &[add_ix(attacker_state, attacker.pubkey(), attacker.pubkey())],
        &payer,
        &[&payer, &attacker],
    );

    // Passes against the attacker's own list...
    assert!(send(
        &mut ctx.svm,
        &[check_ix(attacker_state, attacker.pubkey())],
        &payer,
        &[&payer]
    ));
    // ...and is still refused by the real one.
    assert!(!send(
        &mut ctx.svm,
        &[check_ix(ctx.guard_state, attacker.pubkey())],
        &payer,
        &[&payer]
    ));
}
