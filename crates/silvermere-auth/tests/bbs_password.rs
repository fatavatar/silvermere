//! An account imported with its Worldgroup BBS password (`AccountFlags::BBS_PASSWORD`) is
//! compared as the host compared it: the first 9 characters typed, any case. Setting a
//! password makes it silvermere's own again, compared whole.

use std::sync::Arc;

use silvermere_auth::login::Refusal;
use silvermere_auth::password::{self, Cost, bbs_form};
use silvermere_auth::{AccountFlags, LockoutConfig, LoginService, Role, SystemClock, cli};
use silvermere_db::TestDb;

#[test]
fn the_host_compared_nine_characters_in_any_case() {
    assert_eq!(bbs_form("yourmother"), "yourmothe");
    assert_eq!(bbs_form("YourMothe"), "yourmothe");
    assert_eq!(bbs_form("dragon"), "dragon");
    assert_eq!(bbs_form(""), "");
}

#[tokio::test]
async fn an_imported_password_takes_the_first_nine_characters_in_any_case() {
    let t = TestDb::from_env().await.expect("test database");
    let db = t.db();
    cli::create_with_cost(&**db, "Fatty", "unused", &[Role::Player], Cost::Low)
        .await
        .unwrap();
    // as the import writes it: the stored `yourmothe`, hashed in the compared form
    let row = db.accounts().by_username("Fatty").await.unwrap().unwrap();
    let mut account = row.account;
    account.password_hash = password::hash_with(&bbs_form("yourmothe"), Cost::Low);
    account.flags |= AccountFlags::BBS_PASSWORD.bits();
    db.accounts().update(row.id, &account).await.unwrap();

    let svc = LoginService::new(db.clone(), Arc::new(SystemClock), LockoutConfig::default());
    // each success holds the account's one session: released before the next try
    let ok = async |typed: &str| match svc.authenticate("Fatty", typed).await.unwrap() {
        Ok(account) => {
            svc.online().release(account.id);
            true
        }
        Err(_) => false,
    };
    for typed in ["yourmother", "yourmothe", "YOURMOTHER", "yourmotherlode"] {
        assert!(ok(typed).await, "{typed}");
    }
    for typed in ["yourmoth", "yourmothx", "your mother", ""] {
        assert_eq!(
            svc.authenticate("Fatty", typed).await.unwrap().unwrap_err(),
            Refusal::InvalidCredentials,
            "{typed}"
        );
    }

    // a password set with `account passwd` is compared whole, and its case counts
    cli::set_password_with_cost(&**db, "Fatty", "Yourmother2", Cost::Low)
        .await
        .unwrap();
    let flags = db
        .accounts()
        .by_username("Fatty")
        .await
        .unwrap()
        .unwrap()
        .account
        .flags;
    assert_eq!(flags & AccountFlags::BBS_PASSWORD.bits(), 0);
    assert!(ok("Yourmother2").await);
    for typed in ["Yourmothe", "yourmother2"] {
        assert!(!ok(typed).await, "{typed}");
    }
}
