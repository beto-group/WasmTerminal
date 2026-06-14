use std::io::Write;

#[no_mangle]
pub extern "C" fn init() {
    println!("\x1b[36mWelcome to WASI Shell\x1b[0m");
    println!("Type 'help' to see available commands.");
    print_prompt();
}

static mut CMD_BUFFER: String = String::new();
static mut CWD: String = String::new();

extern "C" {
    fn dispatch_fs_op(op_type: i32, path_ptr: *const u8, path_len: usize);
}

#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

#[no_mangle]
pub extern "C" fn handle_char(c: u32) {
    let ch = std::char::from_u32(c).unwrap_or('?');
    unsafe {
        if ch == '\r' || ch == '\n' {
            println!();
            execute_command(&CMD_BUFFER);
            CMD_BUFFER.clear();
        } else if ch == '\x08' || ch == '\x7f' { // Backspace
            if CMD_BUFFER.pop().is_some() {
                print!("\x08 \x08"); // Erase char visually
                let _ = std::io::stdout().flush();
            }
        } else {
            CMD_BUFFER.push(ch);
            print!("{}", ch);
            let _ = std::io::stdout().flush();
        }
    }
}

fn resolve_path(path: &str) -> String {
    unsafe {
        if CWD.is_empty() {
            CWD = "/".to_string();
        }
        if path.starts_with('/') {
            clean_path(path)
        } else {
            let base = if CWD.ends_with('/') {
                CWD.clone()
            } else {
                format!("{}/", CWD)
            };
            clean_path(&format!("{}{}", base, path))
        }
    }
}

fn clean_path(path: &str) -> String {
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            parts.pop();
        } else {
            parts.push(part);
        }
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn execute_command(cmd: &str) {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        print_prompt();
        return;
    }
    
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let program = parts[0];
    let args = &parts[1..];
    
    match program {
        "help" => {
            println!("Available commands: help, echo, clear, pwd, ls, cd, cat, whoami");
            print_prompt();
        }
        "echo" => {
            println!("{}", args.join(" "));
            print_prompt();
        }
        "clear" => {
            print!("\x1b[2J\x1b[H");
            let _ = std::io::stdout().flush();
            print_prompt();
        }
        "whoami" => {
            println!("datacore-admin");
            print_prompt();
        }
        "pwd" => {
            unsafe {
                if CWD.is_empty() {
                    CWD = "/".to_string();
                }
                println!("{}", CWD);
            }
            print_prompt();
        }
        "ls" => {
            let target = if args.is_empty() {
                "."
            } else {
                args[0]
            };
            let resolved = resolve_path(target);
            unsafe {
                dispatch_fs_op(1, resolved.as_ptr(), resolved.len());
            }
        }
        "cat" => {
            if args.is_empty() {
                println!("cat: missing file operand");
                print_prompt();
            } else {
                let resolved = resolve_path(args[0]);
                unsafe {
                    dispatch_fs_op(2, resolved.as_ptr(), resolved.len());
                }
            }
        }
        "cd" => {
            let target = if args.is_empty() {
                "/"
            } else {
                args[0]
            };
            let resolved = resolve_path(target);
            unsafe {
                dispatch_fs_op(3, resolved.as_ptr(), resolved.len());
            }
        }
        _ => {
            println!("Command not found: {}", program);
            print_prompt();
        }
    }
}

#[no_mangle]
pub extern "C" fn post_fs_result(op_type: i32, status_code: i32, data_ptr: *const u8, data_len: usize) {
    let data = unsafe {
        let slice = std::slice::from_raw_parts(data_ptr, data_len);
        std::str::from_utf8(slice).unwrap_or("")
    };

    match op_type {
        1 => { // ls
            if status_code == 0 {
                if data.is_empty() {
                    println!("(empty directory)");
                } else {
                    println!("{}", data);
                }
            } else {
                println!("ls: error: {}", data);
            }
        }
        2 => { // cat
            if status_code == 0 {
                println!("{}", data);
            } else {
                println!("cat: error: {}", data);
            }
        }
        3 => { // cd
            if status_code == 0 {
                unsafe {
                    CWD = data.to_string();
                }
            } else {
                println!("cd: error: {}", data);
            }
        }
        _ => {}
    }
    print_prompt();
}

fn print_prompt() {
    unsafe {
        if CWD.is_empty() {
            CWD = "/".to_string();
        }
        print!("{} > ", CWD);
        let _ = std::io::stdout().flush();
    }
}
