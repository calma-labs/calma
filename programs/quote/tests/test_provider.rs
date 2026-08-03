//! `quote`'s own behavior.
//!
//! Until now this program had no tests at all. It is only reached by
//! `programs/calma/tests/test_alternative_provider.rs`, which exists to prove
//! that a market can run on a program `calma` never linked — so it asserts
//! things about `calma`, and treats `quote` as an opaque counterparty.
//!
//! The property tested here is the one that makes that possible: the account
//! this program writes must be readable as a `PriceFeedHeader` by a consumer
//! that knows nothing else about it. If the header ever stops being first, every
//! market served by this provider starts reading the wrong bytes as its price —
//! and no test in `calma` would notice, because `calma` cannot see this layout.

use anchor_lang::{
    prelude::Pubkey, solana_program::instruction::Instruction, AnchorDeserialize, AnchorSerialize,
    Discriminator, InstructionData, ToAccountMetas,
};
use anchor_spl::token::spl_token;
use interface::PriceFeedHeader;
use litesvm::LiteSVM;
use quote::state::Provider;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

const QUOTE_SO: &[u8] = include_bytes!("../../../target/deploy/quote.so");

const TTL_MS: u32 = 90_000;
const FLAT_RATE_BPS: u32 = 420;
const PRICE: u64 = 1_000_000;

struct Ctx {
    svm: LiteSVM,
    payer: Keypair,
    authority: Keypair,
    pool: Pubkey,
    provider: Pubkey,
    col_mint: Pubkey,
    lend_mint: Pubkey,
}

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

fn create_mint(svm: &mut LiteSVM, payer: &Keypair, decimals: u8) -> Pubkey {
    use anchor_lang::solana_program::program_pack::Pack;
    let kp = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    let ixs = [
        anchor_lang::solana_program::system_instruction::create_account(
            &payer.pubkey(),
            &kp.pubkey(),
            rent,
            spl_token::state::Mint::LEN as u64,
            &spl_token::id(),
        ),
        spl_token::instruction::initialize_mint(
            &spl_token::id(),
            &kp.pubkey(),
            &payer.pubkey(),
            None,
            decimals,
        )
        .unwrap(),
    ];
    let tx = build(svm, &ixs, payer, &[payer, &kp]);
    svm.send_transaction(tx).expect("create mint");
    kp.pubkey()
}

fn setup() -> Ctx {
    let payer = Keypair::new();
    let authority = Keypair::new();
    let pool = Pubkey::new_unique();
    let mut svm = LiteSVM::new();
    svm.add_program(quote::id(), QUOTE_SO).unwrap();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let col_mint = create_mint(&mut svm, &payer, 6);
    let lend_mint = create_mint(&mut svm, &payer, 6);
    let provider =
        Pubkey::find_program_address(&[b"irm_config", pool.as_ref()], &quote::id()).0;

    Ctx {
        svm,
        payer,
        authority,
        pool,
        provider,
        col_mint,
        lend_mint,
    }
}

fn initialize_ix(ctx: &Ctx, ttl_ms: u32, rate_bps: u32, coll: u64, lend: u64) -> Instruction {
    Instruction::new_with_bytes(
        quote::id(),
        &quote::instruction::Initialize {
            price_ttl_ms: ttl_ms,
            flat_rate_bps: rate_bps,
            collateral_price: coll,
            lend_price: lend,
        }
        .data(),
        quote::accounts::Initialize {
            provider: ctx.provider,
            pool: ctx.pool,
            collateral_mint: ctx.col_mint,
            lend_mint: ctx.lend_mint,
            authority: ctx.authority.pubkey(),
            payer: ctx.payer.pubkey(),
            system_program: anchor_lang::solana_program::system_program::id(),
        }
        .to_account_metas(None),
    )
}

fn setup_initialized() -> Ctx {
    let mut ctx = setup();
    let ix = initialize_ix(&ctx, TTL_MS, FLAT_RATE_BPS, PRICE, PRICE);
    let (payer, authority) = (ctx.payer.insecure_clone(), ctx.authority.insecure_clone());
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &authority]);
    ctx.svm.send_transaction(tx).expect("initialize should succeed");
    ctx
}

fn raw_account(ctx: &Ctx) -> Vec<u8> {
    ctx.svm.get_account(&ctx.provider).unwrap().data
}

// ── the oracle half of the contract ──────────────────────────────────────────

/// The property the whole pluggable-oracle design rests on: a consumer skips the
/// 8-byte discriminator and deserializes a `PriceFeedHeader`, knowing nothing
/// else about the account. Read the bytes exactly the way
/// `interface::read_price_feed` does, rather than through this program's own
/// `Provider` type — going through `Provider` would prove only that the program
/// agrees with itself.
#[test]
fn the_account_reads_as_a_price_feed_header_to_a_consumer_that_knows_nothing_else() {
    let ctx = setup_initialized();
    let data = raw_account(&ctx);

    let mut cursor = &data[8..];
    let header = PriceFeedHeader::deserialize(&mut cursor).expect("header must decode");

    assert_eq!(header.collateral_mint, ctx.col_mint);
    assert_eq!(header.lend_mint, ctx.lend_mint);
    assert_eq!(header.collateral_price, PRICE);
    assert_eq!(header.lend_price, PRICE);
    assert_eq!(header.price_ttl_ms, TTL_MS);
    // Equal prices and decimals means parity, computed by the one shared copy of
    // the formula in `interface`.
    assert_eq!(header.price_ratio(), Some(interface::PRICE_SCALE as u64));

    // And there is more account after the header — the rate-model fields a price
    // account would not have. That tail is what consumers must ignore.
    assert!(
        !cursor.is_empty(),
        "the provider's own fields should follow the header"
    );
}

/// A stricter statement of the same thing: the header occupies the account
/// immediately after the discriminator, at the same offset a bare header would.
#[test]
fn the_header_sits_at_offset_eight_with_nothing_in_front_of_it() {
    let ctx = setup_initialized();
    let data = raw_account(&ctx);

    let mut cursor = &data[8..];
    let header = PriceFeedHeader::deserialize(&mut cursor).unwrap();

    let mut expected = Provider::DISCRIMINATOR.to_vec();
    header.serialize(&mut expected).unwrap();
    assert_eq!(
        &data[..expected.len()],
        expected.as_slice(),
        "discriminator then header, byte for byte"
    );
}

// ── initialize gates ─────────────────────────────────────────────────────────

#[test]
fn initialize_refuses_a_ttl_of_zero() {
    // `0` is the fail-closed sentinel every consumer reads, so a provider created
    // with it could never be borrowed against — better to refuse at creation.
    let mut ctx = setup();
    let ix = initialize_ix(&ctx, 0, FLAT_RATE_BPS, PRICE, PRICE);
    let (payer, authority) = (ctx.payer.insecure_clone(), ctx.authority.insecure_clone());
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &authority]);
    assert!(ctx.svm.send_transaction(tx).is_err());
}

#[test]
fn initialize_refuses_a_zero_price_on_either_side() {
    for (coll, lend) in [(0, PRICE), (PRICE, 0)] {
        let mut ctx = setup();
        let ix = initialize_ix(&ctx, TTL_MS, FLAT_RATE_BPS, coll, lend);
        let (payer, authority) = (ctx.payer.insecure_clone(), ctx.authority.insecure_clone());
        let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &authority]);
        assert!(
            ctx.svm.send_transaction(tx).is_err(),
            "zero price ({coll}, {lend}) must be refused"
        );
    }
}

#[test]
fn initialize_refuses_a_rate_above_the_providers_own_ceiling() {
    let mut ctx = setup();
    let ix = initialize_ix(&ctx, TTL_MS, quote::MAX_RATE_BPS + 1, PRICE, PRICE);
    let (payer, authority) = (ctx.payer.insecure_clone(), ctx.authority.insecure_clone());
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &authority]);
    assert!(ctx.svm.send_transaction(tx).is_err());
}

// ── the rate half of the contract ────────────────────────────────────────────

/// A flat rate is the point: `calma` consumes a `u32` and never learns how it
/// was produced, so "a curve" is not a shape the protocol imposes.
#[test]
fn borrow_rate_returns_the_same_rate_at_every_utilization() {
    let mut ctx = setup_initialized();
    let payer = ctx.payer.insecure_clone();

    for utilization in [0u64, 5_000, 10_000] {
        let ix = Instruction::new_with_bytes(
            quote::id(),
            &quote::instruction::BorrowRate {
                utilization_bps: utilization,
            }
            .data(),
            quote::accounts::RateQuery {
                provider: ctx.provider,
                pool: ctx.pool,
            }
            .to_account_metas(None),
        );
        let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer]);
        let meta = ctx.svm.send_transaction(tx).expect("borrow_rate");
        let got = u32::from_le_bytes(meta.return_data.data[..4].try_into().unwrap());
        assert_eq!(got, FLAT_RATE_BPS, "flat rate at {utilization} bps");
    }
}

#[test]
fn check_authority_accepts_only_the_key_that_claimed_the_provider() {
    let mut ctx = setup_initialized();
    let payer = ctx.payer.insecure_clone();

    let check = |who: Pubkey| {
        Instruction::new_with_bytes(
            quote::id(),
            &quote::instruction::CheckAuthority { authority: who }.data(),
            quote::accounts::RateQuery {
                provider: ctx.provider,
                pool: ctx.pool,
            }
            .to_account_metas(None),
        )
    };

    let ix = check(ctx.authority.pubkey());
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer]);
    assert!(ctx.svm.send_transaction(tx).is_ok());

    let ix = check(Pubkey::new_unique());
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer]);
    assert!(ctx.svm.send_transaction(tx).is_err());
}

// ── writes ───────────────────────────────────────────────────────────────────

#[test]
fn only_the_authority_may_move_the_price() {
    let mut ctx = setup_initialized();
    let payer = ctx.payer.insecure_clone();
    let authority = ctx.authority.insecure_clone();
    let intruder = Keypair::new();
    ctx.svm.airdrop(&intruder.pubkey(), 1_000_000_000).unwrap();

    let set_price = |signer: &Keypair, coll: u64| {
        Instruction::new_with_bytes(
            quote::id(),
            &quote::instruction::SetPrice {
                collateral_price: coll,
                lend_price: PRICE,
            }
            .data(),
            quote::accounts::SetValue {
                provider: ctx.provider,
                authority: signer.pubkey(),
            }
            .to_account_metas(None),
        )
    };

    let ix = set_price(&intruder, 9_000_000);
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &intruder]);
    assert!(
        ctx.svm.send_transaction(tx).is_err(),
        "an outsider must not be able to reprice a market's collateral"
    );

    let ix = set_price(&authority, 2_000_000);
    let tx = build(&mut ctx.svm, &[ix], &payer, &[&payer, &authority]);
    ctx.svm.send_transaction(tx).expect("the authority may reprice");

    let data = raw_account(&ctx);
    let header = PriceFeedHeader::deserialize(&mut &data[8..]).unwrap();
    assert_eq!(header.collateral_price, 2_000_000);
    assert_eq!(header.price_ratio(), Some(2 * interface::PRICE_SCALE as u64));
}
