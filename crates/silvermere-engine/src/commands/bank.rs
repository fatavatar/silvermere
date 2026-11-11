//! Banks: DEPOSIT, WITHDRAW, BANKBOOK/BALANCE, and the BORROW and REPAY stubs.
//!
//! The original's BBS-credit loans are not carried: its BORROW and REPAY do nothing a player
//! can see, so here both words are [`Handled::NotCommand`] and fall through to the exits and
//! speech like any other unknown command. `loan` stays on the record for imported
//! data and is never written.
//!
//! A bankbook is made when a deposit first touches it, so the engine holds a book only after a
//! deposit: a lookup of a missing one is a balance of 0, which is what the original shows.

use std::collections::HashMap;

use silvermere_rules::economy;
use silvermere_text::Msg;
use silvermere_types::enums::{RoomType, ShopType};
use silvermere_types::flags::{PlayerStatus, PlayerStatus2};
use silvermere_types::ids::{AccountId, RoomKey, SessionId, ShopNo};
use silvermere_types::records::BankBook;

use super::dispatch::Handled;
use super::settings::coins_handed_over;
use super::{Cmd, CommandLine};
use crate::display::bank::{display_bank_balance, display_users_bankbooks, proper_currency_name};
use crate::event_loop::{EngineOutput, PersistRequest};
use crate::player::save::{SavePoint, save_at};
use crate::state::newchar::leading_int;
use crate::tell::{Noise, TellBlind, tell_room_led};
use crate::world::World;

/// A deposit the bank cannot take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepositError {
    /// The balance would pass the most a bankbook holds ([`money_limit`]).
    Overflow,
}

/// The most copper a bankbook holds: the original's unsigned 32 bits, or with
/// `game.wide_money` what the database's signed 64-bit column holds.
pub fn money_limit(world: &World) -> u64 {
    if world.config.wide_money {
        i64::MAX as u64
    } else {
        u64::from(u32::MAX)
    }
}

/// A withdrawal the bank cannot pay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithdrawError {
    /// No bankbook at this bank.
    NoBook,
    /// The balance is smaller than the amount.
    Short,
}

/// The bankbooks of every account, `(account, bank)` to book (`World::bank_books`).
pub trait BankBooks {
    /// The book of `acct` at `bank`, if a deposit ever made one.
    fn book(&self, acct: AccountId, bank: ShopNo) -> Option<&BankBook>;
    /// Every book of `acct`, in bank order (a scan of the bankbook file's first key).
    fn for_account(&self, acct: AccountId) -> Vec<&BankBook>;
    /// The balance at `bank`: 0 when there is no book, and capped to `cap` copper when
    /// `cap` is not 0.
    fn balance_at(&mut self, acct: AccountId, bank: ShopNo, cap: u64) -> u64;
    /// Adds `copper` to the balance unless it would pass `limit` ([`money_limit`]); makes the
    /// book (owner `userid`) when there is none. Returns the new balance.
    fn deposit(
        &mut self,
        acct: AccountId,
        userid: &str,
        bank: ShopNo,
        copper: u64,
        limit: u64,
    ) -> Result<u64, DepositError>;
    /// Takes `copper` off the balance when the book holds enough. Returns the new balance.
    fn withdraw(
        &mut self,
        acct: AccountId,
        bank: ShopNo,
        copper: u64,
    ) -> Result<u64, WithdrawError>;
    /// A debit with no message, for the gang house tax (T-056); the same rules as
    /// [`withdraw`](Self::withdraw).
    fn debit(&mut self, acct: AccountId, bank: ShopNo, copper: u64) -> Result<u64, WithdrawError> {
        self.withdraw(acct, bank, copper)
    }
}

impl BankBooks for HashMap<(AccountId, ShopNo), BankBook> {
    fn book(&self, acct: AccountId, bank: ShopNo) -> Option<&BankBook> {
        self.get(&(acct, bank))
    }

    fn for_account(&self, acct: AccountId) -> Vec<&BankBook> {
        let mut books: Vec<&BankBook> = self
            .iter()
            .filter(|((a, _), _)| *a == acct)
            .map(|(_, b)| b)
            .collect();
        books.sort_by_key(|b| b.bank);
        books
    }

    fn balance_at(&mut self, acct: AccountId, bank: ShopNo, cap: u64) -> u64 {
        let Some(book) = self.get_mut(&(acct, bank)) else {
            return 0;
        };
        if cap > 0 && book.balance > cap {
            book.balance = cap;
        }
        book.balance
    }

    fn deposit(
        &mut self,
        acct: AccountId,
        userid: &str,
        bank: ShopNo,
        copper: u64,
        limit: u64,
    ) -> Result<u64, DepositError> {
        let have = self.get(&(acct, bank)).map_or(0, |b| b.balance);
        let balance = have
            .checked_add(copper)
            .filter(|b| *b <= limit)
            .ok_or(DepositError::Overflow)?;
        let book = self.entry((acct, bank)).or_insert_with(|| BankBook {
            userid: userid.to_string(),
            bank,
            balance: 0,
            loan: 0,
        });
        book.balance = balance;
        Ok(balance)
    }

    fn withdraw(
        &mut self,
        acct: AccountId,
        bank: ShopNo,
        copper: u64,
    ) -> Result<u64, WithdrawError> {
        let book = self.get_mut(&(acct, bank)).ok_or(WithdrawError::NoBook)?;
        if book.balance < copper {
            return Err(WithdrawError::Short);
        }
        book.balance -= copper;
        Ok(book.balance)
    }
}

/// The room's bank, if the player stands in one: a shop room whose shop is a bank.
pub fn bank_here(world: &World, id: SessionId) -> Option<ShopNo> {
    let plr = world.session(id)?.player()?;
    let room = world.room(RoomKey {
        map: plr.map,
        room: plr.room,
    })?;
    if room.room_type != RoomType::Shop {
        return None;
    }
    let shop = world.shops.get(&room.shopnum)?;
    (shop.shop_type == ShopType::Bank).then_some(shop.number)
}

/// The amount typed: more than nine characters counts as 0 (so the long cannot overflow),
/// otherwise `leading_int32`, stored in an unsigned long. With `wide` (`game.wide_money`) up
/// to eighteen characters are read, and a negative amount is 0.
fn amount_of(word: &str, wide: bool) -> u64 {
    if wide {
        return if word.len() > 18 {
            0
        } else {
            u64::try_from(leading_int(word)).unwrap_or(0)
        };
    }
    if word.len() > 9 {
        0
    } else {
        u64::from(leading_int(word) as i32 as u32)
    }
}

/// Which way the money goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Way {
    Deposit,
    Withdraw,
}

/// DEPOSIT and WITHDRAW, which differ only in their words and in the argument count they
/// accept (at least two words against exactly two).
fn cmd_banking(world: &mut World, id: SessionId, line: &CommandLine, way: Way) -> Handled {
    let Some(plr) = world.session_mut(id).and_then(|s| s.character.as_mut()) else {
        return Handled::NotCommand;
    };
    // doing anything ends resting and meditating
    plr.status.remove(PlayerStatus::RESTING);
    plr.status2.remove(PlayerStatus2::MEDITATING);
    let Some(bank) = bank_here(world, id) else {
        let msg = match way {
            Way::Deposit => Msg::BankNotDeposit,
            Way::Withdraw => Msg::BankNotWithdraw,
        };
        push(world, id, msg);
        return Handled::Done;
    };
    let wrong_count = match way {
        Way::Deposit => line.argc() < 2,
        Way::Withdraw => line.argc() != 2,
    };
    if wrong_count {
        let msg = match way {
            Way::Deposit => Msg::BankSyntaxDeposit,
            Way::Withdraw => Msg::BankSyntaxWithdraw,
        };
        push(world, id, msg);
        return Handled::Done;
    }
    // doing business ends sneaking and hiding
    if let Some(plr) = world.session_mut(id).and_then(|s| s.character.as_mut()) {
        plr.status.remove(PlayerStatus::SNEAKING);
        plr.hidden = 0;
    }
    let amount = amount_of(line.arg(1), world.config.wide_money);
    if amount == 0 {
        push(world, id, Msg::BankReasonable);
        return Handled::Done;
    }
    match way {
        Way::Deposit => deposit_gold(world, id, bank, amount),
        Way::Withdraw => withdraw_gold(world, id, bank, amount),
    }
    // The dispatcher's colour reset waits in the session's output. When the money moved, the
    // user's line takes it along; when it did not, nothing is told, the reset is taken off the
    // user's output and the others get it ahead of the room line (m3/bank on the original:
    // `deposit 5` with no coins differs only in colour).
    let held = world
        .session_mut(id)
        .is_some_and(|s| s.output.discard_last(&Msg::CmdNormal));
    // the room sees it, whether or not the money moved
    let Some(plr) = world.session(id).and_then(|s| s.player()) else {
        return Handled::Done;
    };
    let (room, who) = (
        RoomKey {
            map: plr.map,
            room: plr.room,
        },
        plr.firstname.clone(),
    );
    let msg = match way {
        Way::Deposit => Msg::BankSeeDeposit { who },
        Way::Withdraw => Msg::BankSeeWithdrawal { who },
    };
    tell_room_led(
        world,
        room,
        [Some(id), None],
        Noise::Always,
        TellBlind::All,
        true,
        None,
        held.then_some(&Msg::CmdNormal),
        &msg,
    );
    Handled::Done
}

fn push(world: &mut World, id: SessionId, msg: Msg) {
    if let Some(s) = world.session_mut(id) {
        s.output.push(msg);
    }
}

/// Queues the book for the persistence worker (marked dirty, saved later).
fn persist_book(world: &mut World, acct: AccountId, bank: ShopNo) {
    if let Some(book) = world.bank_books.get(&(acct, bank)).cloned() {
        world
            .outbox
            .push(EngineOutput::Persist(PersistRequest::BankBook {
                account: acct,
                book,
            }));
    }
}

/// A deposit. Nothing happens, and nothing is shown, when the purse cannot pay; a balance
/// that would wrap is refused with a line and the coins stay.
fn deposit_gold(world: &mut World, id: SessionId, bank: ShopNo, amount: u64) {
    let currency = world.config.usr_cur_name.clone();
    let cap = world.config.maximum_bank_balance;
    let wide = world.config.wide_money;
    let limit = money_limit(world);
    let Some(s) = world.sessions.get(&id) else {
        return;
    };
    let (Some(acct), Some(plr)) = (s.account.as_ref(), s.character.as_ref()) else {
        return;
    };
    let (acct, userid) = (acct.id, acct.username.clone());
    // the original's amounts fit 32 bits (nine digits at most)
    let pay = |coins| {
        if wide {
            economy::pay_wide(coins, amount)
        } else {
            economy::pay(coins, u32::try_from(amount).unwrap_or(u32::MAX))
        }
    };
    if pay(plr.coins).is_none() {
        return;
    }
    world.bank_books.balance_at(acct, bank, cap);
    let msg = match world.bank_books.deposit(acct, &userid, bank, amount, limit) {
        Ok(_) => {
            let plr = world.session_mut(id).and_then(|s| s.character.as_mut());
            let Some(plr) = plr else { return };
            let paid = pay(plr.coins).expect("checked above");
            plr.coins = paid.left;
            persist_book(world, acct, bank);
            Msg::BankDeposit {
                coins: coins_handed_over(paid.paid, &currency),
            }
        }
        Err(DepositError::Overflow) => Msg::BankDepositRefused,
    };
    push(world, id, msg);
    finish(world, id, SavePoint::BankDeposit);
}

/// A withdrawal. Nothing happens, and nothing is shown, when there is no bankbook here or
/// it holds less than the amount.
fn withdraw_gold(world: &mut World, id: SessionId, bank: ShopNo, amount: u64) {
    let cap = world.config.maximum_bank_balance;
    let wide = world.config.wide_money;
    let Some(acct) = world
        .session(id)
        .and_then(|s| s.account.as_ref())
        .map(|a| a.id)
    else {
        return;
    };
    world.bank_books.balance_at(acct, bank, cap);
    // with wide money the coins come as the fewest coins, and a purse that cannot hold them
    // (a coin count past 32 bits) takes nothing
    let paid_out = match world.session(id).and_then(|s| s.player()) {
        Some(plr) if wide => match economy::add_copper_wide(plr.coins, amount) {
            Some(coins) => Some(coins),
            None => return,
        },
        _ => None,
    };
    if world.bank_books.withdraw(acct, bank, amount).is_err() {
        return;
    }
    if let Some(plr) = world.session_mut(id).and_then(|s| s.character.as_mut()) {
        match paid_out {
            Some(coins) => plr.coins = coins,
            // the original's amounts fit 32 bits (nine digits at most)
            None => plr.coins.copper = plr.coins.copper.wrapping_add(amount as u32),
        }
    }
    persist_book(world, acct, bank);
    push(
        world,
        id,
        Msg::BankWithdraw {
            amount,
            coin: proper_currency_name(amount, 0).to_string(),
        },
    );
    finish(world, id, SavePoint::BankWithdraw);
}

/// After a deposit or withdrawal: the purse's coins are normalised and the player saved.
fn finish(world: &mut World, id: SessionId, point: SavePoint) {
    if let Some(plr) = world.session_mut(id).and_then(|s| s.character.as_mut()) {
        plr.coins = economy::normalise(plr.coins);
    }
    save_at(world, id, point);
}

/// DEPOSIT {amount}.
pub fn cmd_deposit(world: &mut World, id: SessionId, _cmd: Cmd, line: &CommandLine) -> Handled {
    cmd_banking(world, id, line, Way::Deposit)
}

/// WITHDRAW {amount}.
pub fn cmd_withdraw(world: &mut World, id: SessionId, _cmd: Cmd, line: &CommandLine) -> Handled {
    cmd_banking(world, id, line, Way::Withdraw)
}

/// BORROW: it is never a command, so BORROW falls through to the room's exits and speech (`borrow
/// skiff` is a text exit at Newhaven Docks).
pub fn cmd_borrow(_world: &mut World, _id: SessionId, _cmd: Cmd, _line: &CommandLine) -> Handled {
    Handled::NotCommand
}

/// REPAY: like [`cmd_borrow`], an empty stub that is never a command.
pub fn cmd_repay(_world: &mut World, _id: SessionId, _cmd: Cmd, _line: &CommandLine) -> Handled {
    Handled::NotCommand
}

/// BANKBOOK and BALANCE. In a bank the plain word shows that bank's balance; elsewhere it lists
/// every bankbook, or `BANKBOOK {n}` shows bank number n (on DOS the number wraps past 32767,
/// so `BANKBOOK 65537` shows bank 1, as in the original).
pub fn cmd_bankbook(world: &mut World, id: SessionId, _cmd: Cmd, line: &CommandLine) -> Handled {
    if world.session(id).and_then(|s| s.player()).is_none() {
        return Handled::NotCommand;
    }
    if let Some(bank) = bank_here(world, id) {
        if line.argc() != 1 {
            push(world, id, Msg::BankSyntaxBankbook);
        } else {
            display_bank_balance(world, id, Some(bank));
        }
    } else if line.argc() != 2 {
        display_users_bankbooks(world, id);
    } else {
        // the cast to int (16 bits on DOS), then to unsigned long: a negative number is no
        // shop
        let n = leading_int(line.arg(1)) as i32;
        let n = if crate::worldgroup::on(world) {
            n
        } else {
            i32::from(n as i16)
        };
        display_bank_balance(world, id, Some(ShopNo(n)));
    }
    Handled::Done
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::parse::parse_command;

    const ACCT: AccountId = AccountId(1);
    const BANK: ShopNo = ShopNo(7);

    // BANKBOOK and BALANCE are one command code (three letters suffice)
    #[test]
    fn bankbook_and_balance_are_one_command() {
        for w in ["BANKBOOK", "BALANCE", "BAN", "BAL", "bankb"] {
            assert_eq!(parse_command(w), Some(Cmd::BankBook), "{w}");
        }
    }

    #[test]
    fn amounts_read_leading_digits_with_a_nine_digit_limit() {
        assert_eq!(amount_of("523", false), 523);
        assert_eq!(amount_of("12abc", false), 12);
        assert_eq!(amount_of("999999999", false), 999_999_999);
        assert_eq!(amount_of("1000000000", false), 0);
        assert_eq!(amount_of("x", false), 0);
        assert_eq!(amount_of("-1", false), u64::from(u32::MAX));
    }

    // game.wide_money: eighteen digits, and a negative amount is none
    #[test]
    fn wide_amounts_read_up_to_eighteen_digits() {
        assert_eq!(amount_of("1000000000", true), 1_000_000_000);
        assert_eq!(
            amount_of("999999999999999999", true),
            999_999_999_999_999_999
        );
        assert_eq!(amount_of("1000000000000000000", true), 0);
        assert_eq!(amount_of("-1", true), 0);
    }

    #[test]
    fn deposit_and_withdraw_limits() {
        let mut b: HashMap<(AccountId, ShopNo), BankBook> = HashMap::new();
        let max = u64::from(u32::MAX);
        assert_eq!(b.withdraw(ACCT, BANK, 1), Err(WithdrawError::NoBook));
        assert_eq!(b.deposit(ACCT, "ANN", BANK, max, max), Ok(max));
        assert_eq!(
            b.deposit(ACCT, "ANN", BANK, 1, max),
            Err(DepositError::Overflow)
        );
        assert_eq!(b.book(ACCT, BANK).unwrap().balance, max);
        // with wide money the book takes it
        assert_eq!(b.deposit(ACCT, "ANN", BANK, 1, u64::MAX), Ok(max + 1));
        assert_eq!(b.withdraw(ACCT, BANK, max + 1), Ok(0));
        assert_eq!(b.withdraw(ACCT, BANK, 1), Err(WithdrawError::Short));
        assert_eq!(b.balance_at(ACCT, ShopNo(9), 0), 0);
    }

    #[test]
    fn books_list_in_bank_order() {
        let mut b: HashMap<(AccountId, ShopNo), BankBook> = HashMap::new();
        for n in [9, 7, 12] {
            b.deposit(ACCT, "ANN", ShopNo(n), 1, u64::MAX).unwrap();
        }
        b.deposit(AccountId(2), "BOB", ShopNo(1), 1, u64::MAX)
            .unwrap();
        let banks: Vec<i32> = b.for_account(ACCT).iter().map(|k| k.bank.0).collect();
        assert_eq!(banks, [7, 9, 12]);
    }
}
