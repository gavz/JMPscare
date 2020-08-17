extern crate clap;
use clap::{Arg, App, SubCommand};
use capstone::prelude::*;
use std::io;
use std::fs::{self, DirEntry};
use std::fs::File;
use std::io::prelude::*;
use std::io::Read;
use std::io::BufRead;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Jump {
    taken: bool,
    not_taken: bool,
    condition: String,
    target: u64,
}

fn read_file(filename: &str) -> Result<String, io::Error> {
    let mut f = File::open(filename)?;
    let mut contents = String::new(); 
    f.read_to_string(&mut contents)?;
    Ok(contents)
}

fn analyze_arm(binary: &Vec<u8>, trace_dir: &str, arch: &str, offset: u64) {
    let mut cs = Capstone::new()
        .arm()
        .mode(arch::arm::ArchMode::Arm)
        .detail(true)
        .build()
        .expect("Failed to create Capstone object for ARM");
    let mut cs_t = Capstone::new()
        .arm()
        .mode(arch::arm::ArchMode::Thumb)
        .detail(true)
        .build()
        .expect("failed to create Capstone object for thumb mode");

    let mut jump_map: HashMap<u64, Jump> = HashMap::new();
    let mut last_jmp_addr: u64 = 0;

    // parse execution traces
    for entry in fs::read_dir(trace_dir).expect("Reading directory contents failed") {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_dir() {
            let curr_file = path.to_str().unwrap();
            let fd = File::open(curr_file).expect("Failed to open file");
            for line in io::BufReader::new(fd).lines() {
                if let Ok(l) = line {
                    let mut addr = u64::from_str_radix(&l.trim_start_matches("0x"), 16).unwrap();
                    let mut disas: capstone::Instructions;
                    if addr % 2 == 0 {
                        disas = cs.disasm_count(&binary[(addr - offset) as usize..], addr, 1).unwrap();
                    } else {
                        // Requirement: addresses of instructions in thumb mode appear with LSB==1 in trace
                        addr -= 1;
                        disas = cs_t.disasm_count(&binary[(addr - offset) as usize..], addr, 1).unwrap();
                    }
                    
                    // check target of last jump
                    if last_jmp_addr != 0 {
                        let last_jmp = jump_map.get_mut(&last_jmp_addr).unwrap();
                        if last_jmp.taken == false && addr == last_jmp.target {
                            last_jmp.taken = true;
                        } else if last_jmp.not_taken == false && addr != last_jmp.target {
                            last_jmp.not_taken = true;
                        }
                        println!("{:?}", last_jmp);

                        last_jmp_addr = 0;
                    }

                    let insn = disas.iter().next().unwrap();
                    if insn.id() == capstone::InsnId(17) { // branch
                        println!("[*] branch");
                        let mnemonic = insn.mnemonic().unwrap();

                        // conditional branch
                        if mnemonic.len() > 2 && mnemonic != "blx" && &mnemonic[1..2] != "." {
                            let t = u64::from_str_radix(&disas
                                .to_string()
                                .split("#0x")
                                .nth(1).unwrap()
                                .trim(), 16).unwrap();
                            println!("{}", t);

                            let new_jmp = Jump {
                                taken: false, 
                                not_taken: false, 
                                condition: String::from(&mnemonic[1..3]), 
                                target: t
                            };

                            jump_map.insert(addr, new_jmp);
                            last_jmp_addr = addr;
                        }
                        
                    }
                }
            }
        }
    }

    // generate output file
    let mut file = File::create("./jxmp_analysis.out".to_string()).expect("Failed to create file");
    for (k, v) in jump_map.iter() {
        println!("{:?}", v);
        if v.taken != v.not_taken {
            let mut s: &str = "";
            if v.taken {
                s = "ALWAYS_TAKEN";
            } else {
                s = "NEVER_TAKEN";
            }
            let line = format!("{:#X} CONDITION_{} {}\n", k, v.condition.to_uppercase(), s);
            file.write(line.as_bytes());
        }
    }
}


fn analyze_x86(binary: &Vec<u8>, trace_dir: &str, arch: &str, offset: u64) {
    let mut cs = Capstone::new()
        .x86()
        .mode(arch::x86::ArchMode::Mode64)
        .detail(true)
        .build()
        .expect("Failed to create Capstone object for x86_64");

    let jump_map: HashMap<u64, Jump> = HashMap::new();

    // parse execution traces
    for entry in fs::read_dir(trace_dir).expect("Reading directory contents failed") {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_dir() {
            let curr_file = path.to_str().unwrap();
            let fd = File::open(curr_file).expect("Failed to open file");
            for line in io::BufReader::new(fd).lines() {
                if let Ok(l) = line {
                    let mut addr = u64::from_str_radix(&l.trim_start_matches("0x"), 16).unwrap();
                    let mut insn: capstone::Instructions;
                    insn = cs.disasm_count(&binary[(addr - offset) as usize..], addr, 1).unwrap();
                    
                    if insn.iter().next().unwrap().id() == capstone::InsnId(17) {
                        println!("We've got a branch");
                        println!("{}", insn);
                    }
                }
            }
        }
    }

    // find uni-directional jumps 
}


fn main() {
    let options = App::new("JXMPscare")
                          .version("0.1")
                          .author("Lukas S. <@pr0me>")
                          .about("Analyze jumps taken across multiple execution traces.")
                          .arg(Arg::with_name("traces")
                               .short("t")
                               .long("traces")
                               .value_name("DIR")
                               .help("Sets path to directory containing collected traces")
                               .required(true)
                               .takes_value(true))
                          .arg(Arg::with_name("arch")
                               .short("a")
                               .long("arch")
                               .value_name("ARCH")
                               .help("Sets binary target architecture. Supported: x86_64, ARM. Default: x86_64")
                               .takes_value(true))
                          .arg(Arg::with_name("base")
                               .short("b")
                               .long("base")
                               .value_name("OFFSET")
                               .help("Sets base address offset. I.e. if the address in a trace is 0x8ffff and the offset is 0x10000, the offset into the 
                                      binary will be 0x7ffff.")
                               .takes_value(true))
                          .arg(Arg::with_name("BINARY")
                               .help("Sets path to original binary the traces were taken from")
                               .required(true)
                               .index(1))
                          .arg(Arg::with_name("v")
                               .short("v")
                               .help("Show verbose output"))
                          .get_matches();

    let bin_path = options.value_of("BINARY").unwrap();
    let trace_path = options.value_of("traces").unwrap();
    let arch = options.value_of("arch").unwrap_or("x86_64");
    let base = u64::from_str_radix(options.value_of("base").unwrap_or("0x00").trim_start_matches("0x"), 16)
        .expect("Failed to parse base offset.");

    let mut f = File::open(options.value_of("BINARY").unwrap()).expect("Failed to open input file.");
    let mut blob = Vec::new();
    f.read_to_end(&mut blob).expect("Failed to read input file.");

    if arch == "ARM" {
        analyze_arm(&blob, trace_path, arch, base);
    } else {
        analyze_x86(&blob, trace_path, arch, base);
    }
    
}