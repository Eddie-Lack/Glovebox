//! Mr. Tip's messages, read from the real game.
//!
//! `GLOVEBOX_ROM` is the ROM's path. Without it the test prints that it was skipped and passes.

use glovebox_core::gamedata;

#[test]
fn mr_tip_says_his_messages() {
    let Ok(rom) = std::env::var("GLOVEBOX_ROM") else {
        println!("mr_tip_says_his_messages: skipped (GLOVEBOX_ROM is not set)");
        return;
    };
    let rom = std::fs::read(rom).expect("the ROM reads");
    let first = gamedata::tip_text(&rom, 0).expect("message 0");
    assert!(first.starts_with("HI, MY NAME IS MR TIP."), "{first}");
    assert!(first.contains("\n\n"), "two pages: {first}");
    let mut n = 0;
    while gamedata::tip_text(&rom, n).is_some() {
        n += 1;
    }
    assert!(n > 60, "{n} messages");
    assert!(gamedata::tip_text(&rom, 10_000).is_none());
    // Every message id a vanilla level uses has a text
    let game = gamedata::GameData::open(rom.clone()).expect("the ROM opens");
    let mut used = 0;
    for s in gamedata::scenes() {
        let xml = game.level_xml(&s.id).unwrap();
        for part in xml.split("message_id=\"").skip(1) {
            let id: usize = part[..part.find('"').unwrap()].parse().unwrap();
            assert!(id < n, "level {} uses message {id}", s.id);
            used += 1;
        }
    }
    assert!(used > 30, "{used} tips in the vanilla levels");
}
