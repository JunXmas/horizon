//! Packet payload oracle companion for tools/test_world_options_parity.py.
use horizon_protocol::{buf::Reader, packets::play::*};
use std::{error::Error, fmt::Write};

pub fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = crate::args().collect();
    let mut output = String::new();
    for line in std::fs::read_to_string(&args[1])?.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let bytes = match fields[0] {
            "difficulty" => Serverbound::ChangeDifficulty(ChangeDifficultyRequest {
                difficulty: fields[1].parse()?,
            })
            .encode(),
            "lock" => Serverbound::LockDifficulty(LockDifficulty {
                locked: fields[1].parse()?,
            })
            .encode(),
            // 1.21.11 has no game rule packets (26.x's set_game_rule, game_rule_values).
            _ => return Err("unknown case".into()),
        };
        let mut reader = Reader::new(&bytes);
        reader.varint()?;
        for byte in reader.rest() {
            write!(output, "{byte:02x}")?;
        }
        output.push('\n');
    }
    std::fs::write(&args[2], output)?;
    Ok(())
}
