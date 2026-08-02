//! The feed's consumption budget: `PriceFeedHeader::price_ttl_ms`.
//!
//! The oracle owns how long its own price stays usable, because it is the only
//! party that knows its update cadence. That makes the field's two guards worth
//! testing directly: it can never be set to the fail-closed `0`, and only the
//! feed's authority can move it — a feed whose TTL anyone could rewrite would
//! let a stranger either brick or indefinitely extend every market pricing
//! against it.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, program_pack::Pack, system_instruction},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::token::spl_token::{self, state::Mint as SplMint},
    feed::state::{Feed, FeedRules, PriceSource},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const FEED_SO: &[u8] = include_bytes!("../../../target/deploy/feed.so");

fn send(svm: &mut LiteSVM, ixs: &[Instruction], signers: &[&Keypair]) -> Result<(), String> {
    svm.expire_blockhash();
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|e| e.meta.logs.join("\n"))
}

#[track_caller]
fn assert_rejected(result: Result<(), String>, expected_error: &str) {
    let logs = result.expect_err("expected the instruction to be rejected");
    assert!(
        logs.contains(expected_error),
        "expected `{expected_error}` but got:\n{logs}"
    );
}

fn create_mint(svm: &mut LiteSVM, payer: &Keypair) -> Pubkey {
    let mint_kp = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(SplMint::LEN);
    let ixs = [
        system_instruction::create_account(
            &payer.pubkey(),
            &mint_kp.pubkey(),
            rent,
            SplMint::LEN as u64,
            &spl_token::id(),
        ),
        spl_token::instruction::initialize_mint(
            &spl_token::id(),
            &mint_kp.pubkey(),
            &payer.pubkey(),
            None,
            6,
        )
        .unwrap(),
    ];
    send(svm, &ixs, &[payer, &mint_kp]).expect("create mint");
    mint_kp.pubkey()
}

struct Ctx {
    svm: LiteSVM,
    authority: Keypair,
    collateral_mint: Pubkey,
    lend_mint: Pubkey,
}

impl Ctx {
    fn feed(&self) -> Pubkey {
        Pubkey::find_program_address(
            &[
                b"feed",
                self.collateral_mint.as_ref(),
                self.lend_mint.as_ref(),
                &[0u8],
            ],
            &feed::id(),
        )
        .0
    }

    fn create(&mut self, price_ttl_ms: u32) -> Result<(), String> {
        let ix = Instruction::new_with_bytes(
            feed::id(),
            &feed::instruction::Create {
                id: 0,
                source: PriceSource::Manual,
                collateral_feed_id: [0u8; 32],
                lend_feed_id: [0u8; 32],
                price_ttl_ms,
                rules: FeedRules::default(),
            }
            .data(),
            feed::accounts::Create {
                feed: self.feed(),
                authority: self.authority.pubkey(),
                collateral_mint: self.collateral_mint,
                lend_mint: self.lend_mint,
                payer: self.authority.pubkey(),
                system_program: anchor_lang::solana_program::system_program::id(),
            }
            .to_account_metas(None),
        );
        let authority = self.authority.insecure_clone();
        send(&mut self.svm, &[ix], &[&authority])
    }

    fn set_price_ttl(&mut self, price_ttl_ms: u32, signer: &Keypair) -> Result<(), String> {
        let ix = Instruction::new_with_bytes(
            feed::id(),
            &feed::instruction::SetPriceTtl { price_ttl_ms }.data(),
            feed::accounts::SetPriceTtl {
                feed: self.feed(),
                authority: signer.pubkey(),
            }
            .to_account_metas(None),
        );
        send(&mut self.svm, &[ix], &[signer])
    }

    fn read_ttl(&self) -> u32 {
        let data = self.svm.get_account(&self.feed()).unwrap().data;
        Feed::try_deserialize(&mut data.as_slice())
            .unwrap()
            .header
            .price_ttl_ms
    }
}

fn setup() -> Ctx {
    let mut svm = LiteSVM::new();
    svm.add_program(feed::id(), FEED_SO)
        .expect("load feed.so — run `anchor build` first");
    let authority = Keypair::new();
    svm.airdrop(&authority.pubkey(), 10_000_000_000).unwrap();
    let collateral_mint = create_mint(&mut svm, &authority);
    let lend_mint = create_mint(&mut svm, &authority);
    Ctx {
        svm,
        authority,
        collateral_mint,
        lend_mint,
    }
}

#[test]
fn create_stores_the_ttl_in_the_header() {
    let mut ctx = setup();
    ctx.create(45_000).unwrap();
    assert_eq!(ctx.read_ttl(), 45_000);
}

/// `0` is the fail-closed sentinel every consumer reads, so a feed created with
/// it could never be borrowed against. Refuse at the source rather than shipping
/// a feed that silently cannot be used.
#[test]
fn create_refuses_a_zero_ttl() {
    let mut ctx = setup();
    assert_rejected(ctx.create(0), "InvalidPriceTtl");
}

/// Feeds outlive the cadence assumptions made when they were created — a
/// publisher that slows down needs its budget widened without the feed being
/// recreated, which would orphan every market pinned to its address.
#[test]
fn the_authority_can_retune_the_ttl() {
    let mut ctx = setup();
    ctx.create(45_000).unwrap();

    let authority = ctx.authority.insecure_clone();
    ctx.set_price_ttl(120_000, &authority).unwrap();
    assert_eq!(ctx.read_ttl(), 120_000);
}

/// The same fail-closed reasoning as `create`: `set_price_ttl(0)` would be a
/// one-instruction kill switch for every market pricing against this feed,
/// dressed up as a configuration change.
#[test]
fn set_price_ttl_refuses_zero() {
    let mut ctx = setup();
    ctx.create(45_000).unwrap();

    let authority = ctx.authority.insecure_clone();
    assert_rejected(ctx.set_price_ttl(0, &authority), "InvalidPriceTtl");
    assert_eq!(ctx.read_ttl(), 45_000, "the rejected write must not land");
}

/// Anyone able to move the TTL could brick every market pricing against this
/// feed, or extend a stale price's life indefinitely.
#[test]
fn a_stranger_cannot_retune_the_ttl() {
    let mut ctx = setup();
    ctx.create(45_000).unwrap();

    let stranger = Keypair::new();
    ctx.svm.airdrop(&stranger.pubkey(), 10_000_000_000).unwrap();

    assert_rejected(ctx.set_price_ttl(1, &stranger), "Unauthorized");
    assert_eq!(ctx.read_ttl(), 45_000);
}
