#![allow(dead_code)]

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::program_pack::Pack;
use {
    anchor_lang::solana_program::instruction::Instruction,
    anchor_spl::token::spl_token,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

pub fn send_ixs(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).expect("transaction failed");
}

pub fn try_send_ixs(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> bool {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).is_ok()
}

pub fn create_mint_ixs(
    payer: &Pubkey,
    mint: &Pubkey,
    authority: &Pubkey,
    rent: u64,
) -> [Instruction; 2] {
    [
        anchor_lang::solana_program::system_instruction::create_account(
            payer,
            mint,
            rent,
            spl_token::state::Mint::LEN as u64,
            &spl_token::id(),
        ),
        spl_token::instruction::initialize_mint(&spl_token::id(), mint, authority, None, 6)
            .unwrap(),
    ]
}

pub fn create_token_account_ixs(
    payer: &Pubkey,
    account: &Pubkey,
    mint: &Pubkey,
    owner: &Pubkey,
    rent: u64,
) -> [Instruction; 2] {
    [
        anchor_lang::solana_program::system_instruction::create_account(
            payer,
            account,
            rent,
            spl_token::state::Account::LEN as u64,
            &spl_token::id(),
        ),
        spl_token::instruction::initialize_account(&spl_token::id(), account, mint, owner).unwrap(),
    ]
}

/// Hands a mint's `MintTokens` authority to `new_authority`.
///
/// `faucet::mock_swap` mints under the faucet's `["mint_authority"]` PDA, so any
/// mint it swaps must be moved over first — after all `mint_to_ix` funding, since
/// the old authority can no longer mint afterwards.
pub fn set_mint_authority_ix(
    mint: &Pubkey,
    current_authority: &Pubkey,
    new_authority: &Pubkey,
) -> Instruction {
    spl_token::instruction::set_authority(
        &spl_token::id(),
        mint,
        Some(new_authority),
        spl_token::instruction::AuthorityType::MintTokens,
        current_authority,
        &[],
    )
    .unwrap()
}

pub fn mint_to_ix(mint: &Pubkey, dest: &Pubkey, authority: &Pubkey, amount: u64) -> Instruction {
    spl_token::instruction::mint_to(&spl_token::id(), mint, dest, authority, &[], amount).unwrap()
}

pub fn read_token_balance(svm: &LiteSVM, account: &Pubkey) -> u64 {
    let data = svm.get_account(account).unwrap().data;
    spl_token::state::Account::unpack(&data).unwrap().amount
}

