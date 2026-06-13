use eframe::egui;
use js_sys::{Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{WebSocket, MessageEvent};
use std::rc::Rc;
use std::cell::RefCell;
use vt100::Parser;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = datacore_fs, catch)]
    async fn list(path: &str) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = datacore_fs, catch)]
    async fn read(path: &str) -> Result<JsValue, JsValue>;
    
    #[wasm_bindgen(js_namespace = datacore_fs, catch)]
    async fn stat(path: &str) -> Result<JsValue, JsValue>;
}

pub struct TerminalApp {
    history: Rc<RefCell<Vec<String>>>,
    input: String,
    cwd: Rc<RefCell<String>>,
    theme_synced: bool,
    
    // PTY Backend
    is_pty: bool,
    ws: Rc<RefCell<Option<WebSocket>>>,
    parser: Rc<RefCell<Parser>>,
}

impl Default for TerminalApp {
    fn default() -> Self {
        Self {
            history: Rc::new(RefCell::new(vec![
                "WasmTerminal v4.0 (Hybrid Edition)".to_string(),
            ])),
            input: String::new(),
            cwd: Rc::new(RefCell::new("/".to_string())),
            theme_synced: false,
            is_pty: false,
            ws: Rc::new(RefCell::new(None)),
            parser: Rc::new(RefCell::new(Parser::new(30, 140, 0))),
        }
    }
}

impl TerminalApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::default();
        app.boot(cc.egui_ctx.clone());
        app
    }

    fn boot(&mut self, ctx: egui::Context) {
        if let Some(window) = web_sys::window() {
            if let Ok(pty_url) = Reflect::get(&window, &JsValue::from_str("datacore_pty_url")) {
                if let Some(url_str) = pty_url.as_string() {
                    self.is_pty = true;
                    self.history.borrow_mut().push("Connecting to Native PTY Server...".to_string());
                    self.connect_pty(&url_str, ctx);
                    return;
                }
            }
        }
        
        self.history.borrow_mut().push("Running in Virtual Shell Mode".to_string());
    }

    fn connect_pty(&mut self, url: &str, ctx: egui::Context) {
        let ws = match WebSocket::new_with_str(url, "tty") {
            Ok(socket) => socket,
            Err(e) => {
                let err_str = e.dyn_into::<js_sys::Error>().map(|e| e.message().into()).unwrap_or_else(|_| "Unknown error".to_string());
                self.history.borrow_mut().push(format!("WebSocket connection failed: {}", err_str));
                return;
            }
        };
        ws.set_binary_type(web_sys::BinaryType::Blob);

        let parser = self.parser.clone();
        
        // Setup onmessage handler
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let s: String = txt.into();
                if s.starts_with('0') {
                    let mut p = parser.borrow_mut();
                    p.process(s[1..].as_bytes());
                    ctx.request_repaint();
                }
            } else if let Ok(blob) = e.data().dyn_into::<web_sys::Blob>() {
                if let Ok(file_reader) = web_sys::FileReader::new() {
                    let fr_c = file_reader.clone();
                    let p_clone = parser.clone();
                    let ctx_clone = ctx.clone();
                    
                    let onload_cb = Closure::wrap(Box::new(move |_e: web_sys::ProgressEvent| {
                        if let Ok(array_buffer) = fr_c.result() {
                            let uint8_array = Uint8Array::new(&array_buffer);
                            let mut bytes = vec![0; uint8_array.length() as usize];
                            uint8_array.copy_to(&mut bytes);
                            
                            if bytes.len() > 0 && bytes[0] == b'0' {
                                let mut p = p_clone.borrow_mut();
                                p.process(&bytes[1..]);
                                ctx_clone.request_repaint();
                            }
                        }
                    }) as Box<dyn FnMut(web_sys::ProgressEvent)>);
                    
                    file_reader.set_onload(Some(onload_cb.as_ref().unchecked_ref()));
                    onload_cb.forget();
                    let _ = file_reader.read_as_array_buffer(&blob);
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();

        let ws_onopen = ws.clone();
        let onopen_callback = Closure::wrap(Box::new(move |_: JsValue| {
            web_sys::console::log_1(&JsValue::from_str("[WasmTerminal] WebSocket Open! Sending AuthToken & Resize..."));
            let init_msg = "{\"AuthToken\":\"\"}";
            let _ = ws_onopen.send_with_str(init_msg);
            
            // Tell ttyd the terminal size so it spawns the shell process
            let resize_msg = "1{\"columns\":140,\"rows\":30}";
            let _ = ws_onopen.send_with_str(resize_msg);
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        *self.ws.borrow_mut() = Some(ws);
    }

    fn send_pty_input(&self, input: &str) {
        if let Some(ws) = self.ws.borrow().as_ref() {
            if ws.ready_state() == 1 {
                let msg = format!("0{}", input);
                let _ = ws.send_with_str(&msg);
            }
        }
    }

    fn sync_obsidian_theme(&mut self, ctx: &egui::Context) {
        if self.theme_synced {
            return;
        }

        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(body) = document.body() {
                    if let Ok(Some(computed_style)) = window.get_computed_style(&body) {
                        let mut visuals = egui::Visuals::dark();

                        if let Ok(bg_color) = computed_style.get_property_value("--background-primary") {
                            if let Some(color) = parse_css_color(&bg_color) {
                                visuals.panel_fill = color;
                                visuals.window_fill = color;
                            }
                        }

                        if let Ok(text_color) = computed_style.get_property_value("--text-normal") {
                            if let Some(color) = parse_css_color(&text_color) {
                                visuals.override_text_color = Some(color);
                            }
                        }

                        ctx.set_visuals(visuals);
                        self.theme_synced = true;
                    }
                }
            }
        }
    }

    // -- Virtual Shell (Mobile Fallback) --
    fn execute_vfs_command(&mut self, _ctx: egui::Context) {
        // ... (Previous VFS command handler code)
        // Kept brief for structural clarity
        let cmd = self.input.trim().to_string();
        if cmd.is_empty() { return; }
        let cwd = self.cwd.borrow().clone();
        self.history.borrow_mut().push(format!("datacore@wasm {} ~$ {}", cwd, cmd));
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let program = parts[0].to_string();
        let _args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        match program.as_str() {
            "help" => self.history.borrow_mut().push("VFS Mode: help, clear, pwd, ls, cd, cat".to_string()),
            "clear" => self.history.borrow_mut().clear(),
            "pwd" => self.history.borrow_mut().push(cwd.clone()),
            "ls" => {
                let target_dir = if _args.is_empty() { cwd.clone() } else {
                    let p = _args[0].clone();
                    if p.starts_with('/') { p } else if cwd == "/" { format!("/{}", p) } else { format!("{}/{}", cwd, p) }
                };
                let history_clone = self.history.clone();
                let ctx_clone = _ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match list(&target_dir).await {
                        Ok(js_val) => {
                            if let Some(json_str) = js_val.as_string() {
                                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
                                    if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) {
                                        history_clone.borrow_mut().push(format!("ls: {}", err));
                                    } else {
                                        let mut out = String::new();
                                        if let Some(folders) = parsed.get("folders").and_then(|v| v.as_array()) {
                                            for f in folders {
                                                if let Some(s) = f.as_str() {
                                                    let name = s.split('/').last().unwrap_or(s);
                                                    out.push_str(&format!("[DIR] {}\n", name));
                                                }
                                            }
                                        }
                                        if let Some(files) = parsed.get("files").and_then(|v| v.as_array()) {
                                            for f in files {
                                                if let Some(s) = f.as_str() {
                                                    let name = s.split('/').last().unwrap_or(s);
                                                    out.push_str(&format!("{} \n", name));
                                                }
                                            }
                                        }
                                        if out.is_empty() { out.push_str("(empty directory)\n"); }
                                        for line in out.lines() {
                                            history_clone.borrow_mut().push(line.to_string());
                                        }
                                    }
                                } else {
                                    history_clone.borrow_mut().push(json_str);
                                }
                            }
                            ctx_clone.request_repaint();
                        },
                        Err(e) => {
                            let err_str = e.dyn_into::<js_sys::Error>().map(|e| e.message().into()).unwrap_or_else(|_| "Unknown error".to_string());
                            history_clone.borrow_mut().push(format!("ls error: {}", err_str));
                            ctx_clone.request_repaint();
                        }
                    }
                });
            },
            "cat" => {
                if _args.is_empty() {
                    self.history.borrow_mut().push("cat: missing file operand".to_string());
                } else {
                    let mut target_file = _args[0].clone();
                    if !target_file.starts_with('/') {
                        if cwd == "/" { target_file = format!("/{}", target_file); }
                        else { target_file = format!("{}/{}", cwd, target_file); }
                    }
                    let history_clone = self.history.clone();
                    let ctx_clone = _ctx.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        match read(&target_file).await {
                            Ok(js_val) => {
                                if let Some(json_str) = js_val.as_string() {
                                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
                                        if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) {
                                            history_clone.borrow_mut().push(format!("cat: {}", err));
                                        } else if let Some(content) = parsed.get("content").and_then(|v| v.as_str()) {
                                            for line in content.lines() {
                                                history_clone.borrow_mut().push(line.to_string());
                                            }
                                        }
                                    } else {
                                        history_clone.borrow_mut().push(json_str);
                                    }
                                }
                                ctx_clone.request_repaint();
                            },
                            Err(e) => {
                                let err_str = e.dyn_into::<js_sys::Error>().map(|e| e.message().into()).unwrap_or_else(|_| "Unknown error".to_string());
                                history_clone.borrow_mut().push(format!("cat error: {}", err_str));
                                ctx_clone.request_repaint();
                            }
                        }
                    });
                }
            },
            "cd" => {
                if _args.is_empty() {
                    *self.cwd.borrow_mut() = "/".to_string();
                } else {
                    let target_dir = _args[0].clone();
                    let new_cwd = if target_dir == "/" {
                        "/".to_string()
                    } else if target_dir == ".." {
                        let mut parts: Vec<&str> = cwd.split('/').filter(|s| !s.is_empty()).collect();
                        if !parts.is_empty() { parts.pop(); }
                        if parts.is_empty() { "/".to_string() } else { format!("/{}", parts.join("/")) }
                    } else if target_dir.starts_with('/') {
                        target_dir
                    } else {
                        if cwd == "/" { format!("/{}", target_dir) }
                        else { format!("{}/{}", cwd, target_dir) }
                    };
                    *self.cwd.borrow_mut() = new_cwd;
                }
            },
            // Simplified fallback
            _ => self.history.borrow_mut().push(format!("command not found: {}", program)),
        }
        self.input.clear();
    }
}

impl eframe::App for TerminalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_obsidian_theme(ctx);

        // Handle Keyboard Input for PTY
        if self.is_pty {
            ctx.input(|i| {
                for event in &i.events {
                    match event {
                        egui::Event::Text(t) => self.send_pty_input(t),
                        egui::Event::Key { key, pressed: true, modifiers, .. } => {
                            if *key == egui::Key::Enter {
                                self.send_pty_input("\r");
                            } else if *key == egui::Key::Backspace {
                                self.send_pty_input("\x7f");
                            } else if *key == egui::Key::Escape {
                                self.send_pty_input("\x1b");
                            } else if *key == egui::Key::Delete {
                                self.send_pty_input("\x1b[3~");
                            } else if *key == egui::Key::Insert {
                                self.send_pty_input("\x1b[2~");
                            } else if *key == egui::Key::Home {
                                self.send_pty_input("\x1b[H");
                            } else if *key == egui::Key::End {
                                self.send_pty_input("\x1b[F");
                            } else if *key == egui::Key::PageUp {
                                self.send_pty_input("\x1b[5~");
                            } else if *key == egui::Key::PageDown {
                                self.send_pty_input("\x1b[6~");
                            } else if *key == egui::Key::ArrowUp {
                                self.send_pty_input("\x1b[A");
                            } else if *key == egui::Key::ArrowDown {
                                self.send_pty_input("\x1b[B");
                            } else if *key == egui::Key::ArrowRight {
                                self.send_pty_input("\x1b[C");
                            } else if *key == egui::Key::ArrowLeft {
                                self.send_pty_input("\x1b[D");
                            } else if *key == egui::Key::Tab {
                                self.send_pty_input("\t");
                            } else if modifiers.ctrl {
                                let key_val = match key {
                                    egui::Key::A => Some(1u8),
                                    egui::Key::B => Some(2u8),
                                    egui::Key::C => Some(3u8),
                                    egui::Key::D => Some(4u8),
                                    egui::Key::E => Some(5u8),
                                    egui::Key::F => Some(6u8),
                                    egui::Key::G => Some(7u8),
                                    egui::Key::H => Some(8u8),
                                    egui::Key::I => Some(9u8),
                                    egui::Key::J => Some(10u8),
                                    egui::Key::K => Some(11u8),
                                    egui::Key::L => Some(12u8),
                                    egui::Key::M => Some(13u8),
                                    egui::Key::N => Some(14u8),
                                    egui::Key::O => Some(15u8),
                                    egui::Key::P => Some(16u8),
                                    egui::Key::Q => Some(17u8),
                                    egui::Key::R => Some(18u8),
                                    egui::Key::S => Some(19u8),
                                    egui::Key::T => Some(20u8),
                                    egui::Key::U => Some(21u8),
                                    egui::Key::V => Some(22u8),
                                    egui::Key::W => Some(23u8),
                                    egui::Key::X => Some(24u8),
                                    egui::Key::Y => Some(25u8),
                                    egui::Key::Z => Some(26u8),
                                    _ => None,
                                };
                                if let Some(val) = key_val {
                                    let c = val as char;
                                    self.send_pty_input(&c.to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.is_pty {
                egui::ScrollArea::both()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        // Render connection status first
                        let history = self.history.borrow();
                        for line in history.iter() {
                            ui.label(egui::RichText::new(line).family(egui::FontFamily::Monospace).color(egui::Color32::from_rgb(150, 150, 150)));
                        }
                        
                        // Render VT100 Grid
                        let parser = self.parser.borrow();
                        let text = parser.screen().contents();
                        ui.add(egui::Label::new(egui::RichText::new(text).family(egui::FontFamily::Monospace)).wrap(false));
                    });
            } else {
                // Render VFS History
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .stick_to_bottom(true)
                    .max_height(ui.available_height() - 30.0)
                    .show(ui, |ui| {
                        let history = self.history.borrow();
                        for line in history.iter() {
                            ui.label(egui::RichText::new(line).family(egui::FontFamily::Monospace));
                        }
                    });

                // VFS Input
                ui.horizontal(|ui| {
                    let cwd = self.cwd.borrow().clone();
                    ui.label(egui::RichText::new(format!("datacore@wasm {} ~$ ", cwd)).family(egui::FontFamily::Monospace).color(egui::Color32::from_rgb(137, 180, 250)));
                    let response = ui.add(egui::TextEdit::singleline(&mut self.input).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).frame(false));
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.execute_vfs_command(ctx.clone());
                        response.request_focus();
                    }
                    if !response.has_focus() { response.request_focus(); }
                });
            }
        });
    }
}

fn parse_css_color(color_str: &str) -> Option<egui::Color32> {
    let s = color_str.trim();
    if s.starts_with('#') {
        let hex = s.trim_start_matches('#');
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(egui::Color32::from_rgb(r, g, b));
        }
    }
    None
}
