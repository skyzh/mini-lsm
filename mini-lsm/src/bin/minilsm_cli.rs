use std::time::Duration;

use anyhow::Result;
use mini_lsm::compact::{CompactionOptions, SimpleLeveledCompactionOptions};
use mini_lsm::lsm_storage::{LsmStorageOptions, MiniLsm};

fn main() -> Result<()> {
    let lsm = MiniLsm::open(
        "mini-lsm.db",
        LsmStorageOptions {
            block_size: 4096,
            target_sst_size: 2 << 20,
            compaction_options: CompactionOptions::Simple(SimpleLeveledCompactionOptions {
                size_ratio_percent: 200,
                level0_file_num_compaction_trigger: 2,
                max_levels: 4,
            }),
        },
    )?;
    let mut epoch = 0;
    loop {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let line = line.trim().to_string();
        if line.starts_with("fill ") {
            let Some((_, options)) = line.split_once(' ') else {
                println!("invalid command");
                continue;
            };
            let Some((begin, end)) = options.split_once(' ') else {
                println!("invalid command");
                continue;
            };
            let begin = begin.parse::<u64>()?;
            let end = end.parse::<u64>()?;

            for i in begin..=end {
                lsm.put(
                    format!("{}", i).as_bytes(),
                    format!("value{}@{}", i, epoch).as_bytes(),
                )?;
            }

            println!("{} values filled with epoch {}", end - begin + 1, epoch);
        } else if line.starts_with("get ") {
            let Some((_, key)) = line.split_once(' ') else {
                println!("invalid command");
                continue;
            };
            if let Some(value) = lsm.get(key.as_bytes())? {
                println!("{}={:?}", key, value);
            } else {
                println!("{} not exist", key);
            }
        } else if line == "flush" {
            lsm.force_flush_imm_memtables()?;
        } else if line == "quit" {
            lsm.close()?;
            break;
        } else {
            println!("invalid command: {}", line);
        }
        epoch += 1;
    }
    Ok(())
}
