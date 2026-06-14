use std::io::Write;

#[no_mangle]
pub extern "C" fn init() {
    println!("\x1b[36mWelcome to WASI Shell\x1b[0m");
    println!("Type 'help' to see available commands.");
    print!("> ");
    let _ = std::io::stdout().flush();
}

static mut CMD_BUFFER: String = String::new();

#[no_mangle]
pub extern "C" fn handle_char(c: u32) {
    let ch = std::char::from_u32(c).unwrap_or('?');
    unsafe {
        if ch == '\r' || ch == '\n' {
            println!();
            execute_command(&CMD_BUFFER);
            CMD_BUFFER.clear();
            print!("> ");
            let _ = std::io::stdout().flush();
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

fn execute_command(cmd: &str) {
    let cmd = cmd.trim();
    if cmd == "help" {
        println!("Available commands: help, echo, clear, whoami");
    } else if cmd.starts_with("echo ") {
        println!("{}", &cmd[5..]);
    } else if cmd == "clear" {
        print!("\x1b[2J\x1b[H");
    } else if cmd == "whoami" {
        println!("datacore-admin");
    } else if !cmd.is_empty() {
        println!("Command not found: {}", cmd);
    }
}
