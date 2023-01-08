pub mod edit;
pub mod info;
pub mod cartdata;
pub mod math;
pub mod resolution;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::fmt;
use std::cmp::{max, PartialOrd};
use std::time::Duration;
use log::{debug, error, log_enabled, info, Level};


#[cfg(feature = "image")]
use image;

#[cfg(feature = "image")]
use gif;

#[cfg(feature = "image")]
use std::path::Path;
#[cfg(feature = "image")]
use std::fs::File;


use plugins::lua_plugin::plugin::LuaPlugin;
use plugins::python_plugin::plugin::PythonPlugin;
use plugins::javascript_plugin::plugin::JavascriptPlugin;

use gfx;
use contexts;
use cartridge::{Cartridge, CartridgeFormat};

include!(concat!(env!("OUT_DIR"), "/parameters.rs"));

#[derive(Clone, Debug, PartialEq)]
pub enum UnicornState {
    STOP,
    RUN,
    PAUSE,
    EDITOR,
}

#[derive(Debug)]
pub enum Code {
    UNKNOWN = 0,
    LUA = 1,
    PYTHON = 2,
    JAVASCRIPT = 3,
    WASM = 4,
}


#[cfg_attr(rustfmt, rustfmt_skip)]
pub fn draw_logo(screen: &mut gfx::Screen) {
    let height = screen.height;

    screen
        .print(format!("Powered by Unicorn {:?}.{:?}.{:?}", VERSION, MAJOR_VERSION, MINOR_VERSION).to_string(),
        0,
        (height-16) as i32,
        7);
}

#[derive(Debug)]
pub struct Record {
    pub recording: bool,
    pub images: Vec<u8>,
    pub filename: String,
    pub nb: i32,
}

impl Record {
    pub fn new() -> Record {
        let images = Vec::new();

        Record {
            recording: false,
            images: images,
            filename: "".to_string(),
            nb: 0,
        }
    }
}


pub struct UnicornCartridge {
    pub filename: String,
    pub full_filename: String,
    
    pub loaded: bool,

    pub font_name: String,
    
    pub cartridge: Cartridge,
    pub lua_plugin: LuaPlugin,
    pub python_plugin: PythonPlugin,
    pub javascript_plugin: JavascriptPlugin,
}


impl UnicornCartridge {
    pub fn new(cartridge: Cartridge, filename: String) -> UnicornCartridge {
        UnicornCartridge {
            filename: filename.clone(),
            full_filename: cartridge.filename.clone(),
            loaded: true,
            font_name: "pico-8".to_string(),
            cartridge: cartridge,
            lua_plugin: LuaPlugin::new(),
            python_plugin: PythonPlugin::new(),
            javascript_plugin: JavascriptPlugin::new(),
        }
    }

    pub fn empty(filename: String, full_filename: String) -> UnicornCartridge {
        UnicornCartridge {
            filename: filename.clone(),
            full_filename: full_filename.clone(),
            loaded: false,
            font_name: "pico-8".to_string(),
            cartridge: Cartridge::empty(),
            lua_plugin: LuaPlugin::new(),
            python_plugin: PythonPlugin::new(),
            javascript_plugin: JavascriptPlugin::new(),
        }
    }

    pub fn zero() -> UnicornCartridge {
        UnicornCartridge {
            filename: "".to_string(),
            full_filename: "".to_string(),
            loaded: false,
            font_name: "pico-8".to_string(),
            cartridge: Cartridge::empty(),
            lua_plugin: LuaPlugin::new(),
            python_plugin: PythonPlugin::new(),
            javascript_plugin: JavascriptPlugin::new(),
        }
    }

    pub fn get_code_type(&mut self) -> Code {
        match self.cartridge.code.get_name().as_ref() {
            "lua" => Code::LUA,
            "python" => Code::PYTHON,
            "javascript" => Code::JAVASCRIPT,
            "wasm" => Code::WASM,
            _ => Code::UNKNOWN,
        }
    }

    pub fn get_code(&mut self) -> String {
        self.cartridge.code.get_data().clone()
    }

    pub fn set_code(&mut self, data: Vec<String>) {
        self.cartridge.code.set_data(data);
    }

    pub fn get_palettes(&mut self) -> HashMap<u32, gfx::palette::RGB> {
        self.cartridge.palette.colors.clone()
    }
}

impl fmt::Debug for UnicornCartridge {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "Unicorn Cartridge {{ cart: {:?} }}",
               self.cartridge)
    }
}


pub struct Unicorn {
    pub screen: Arc<Mutex<gfx::Screen>>,
    pub contexts: Arc<Mutex<contexts::Contexts>>,

    pub info: Arc<Mutex<info::Info>>,
    pub debug: bool,
    pub cartridge: UnicornCartridge,
    pub state: UnicornState,
    pub fps: f64,
    pub record: Record,
    pub update_return: bool,
    pub version: u32,
    pub major_version: u32,
    pub minor_version: u32,
    pub frame_rate: gfx::framerate::FrameRate,
}

impl Unicorn {
    pub fn new() -> Unicorn {
        info!("[Unicorn] Creating new Unicorn");

        let screen = Arc::new(Mutex::new(gfx::Screen::new(MAP_WIDTH, MAP_HEIGHT, 128, 32)));

        Unicorn {
            screen: screen.clone(),
            contexts: Arc::new(Mutex::new(contexts::Contexts::new(2))),

            info: Arc::new(Mutex::new(info::Info::new())),
            debug: false,
            
            cartridge: UnicornCartridge::zero(),
           
            state: UnicornState::STOP,
            
            fps: 0.0,
            record: Record::new(),
            update_return: true,

            version: VERSION,
            major_version: MAJOR_VERSION,
            minor_version: MINOR_VERSION,

            frame_rate: gfx::framerate::FrameRate::default(),
        }
    }

    pub fn width(&mut self) -> u32 {
        MAP_WIDTH as u32
    }

    pub fn height(&mut self) -> u32 {
        MAP_HEIGHT as u32
    }
    
    pub fn is_none(&mut self) -> bool {
        self.state == UnicornState::STOP
    }

    pub fn setup(&mut self) {
        info!("[Unicorn] Setup");
        self.reset();
    }

    pub fn stop(&mut self) {
    }

    pub fn toggle_debug(&mut self) {
        self.debug = !self.debug;
    }

    pub fn reset(&mut self) {
        info!("[Unicorn] Reset");

        self.screen.lock().unwrap().reset();
        self.update_return = true;
    }

    pub fn debug_draw(&mut self) {
        if self.debug {
            let screen = &mut self.screen.lock().unwrap();
            let mouse_x = 0; //self.players.lock().unwrap().mouse_coordinate(0);
            let mouse_y = 0; //self.players.lock().unwrap().mouse_coordinate(1);

            let width = screen.width as i32;
            
            screen.rectfill(0, 0, width, 8, 0);

            screen.force_print(format!("{:.0}FPS {:.2?} {:.2?} {:?} {:?}",
                                       self.fps,
                                       mouse_x,
                                       mouse_y,
                                       "xx",
                                       self.state)
                                       .to_string(),
                               0,
                               0,
                               7);
                               
        }
    }

    pub fn update_time(&mut self) {
        self.info.lock().unwrap().update();
    }

    pub fn init(&mut self) {
        self.state = UnicornState::RUN;
        self.call_init();
    }

    pub fn update(&mut self) -> bool {
        match self.state {
            UnicornState::STOP => {

            }
            UnicornState::PAUSE => {

            }
            UnicornState::RUN => {
                if self.is_end() {
                    return false;
                }

                self.call_update();
            }
            UnicornState::EDITOR => {

            }
        }

        self.update_time();
        true
    }

    pub fn draw(&mut self) {
        match self.state {
            UnicornState::STOP => {

            }
            UnicornState::PAUSE => {
                // self.pause_menu.draw(&mut self.screen.lock().unwrap());
            }
            UnicornState::RUN => {
                self.call_draw();
            }
            UnicornState::EDITOR => {

            }
        }

        if self.is_recording() {
            self.record();
        }

        self.debug_draw();
    }

    pub fn is_end(&self) -> bool {
        !self.update_return
    }

    pub fn is_recording(&self) -> bool {
        self.record.recording
    }

    pub fn start_record(&mut self, filename: &str) {
        info!("[Unicorn] Start to record the frame");

        self.record.recording = true;
        self.record.images.clear();
        self.record.filename = filename.to_string();
    }

    pub fn record(&mut self) {
        /* 
        info!("[Unicorn] Recording the frame {:?}", self.record.images.len());

        if self.record.nb % 4 == 0 {
            let mut buffer: Vec<u8> = Vec::new();
            let screen = &mut self.screen.lock().unwrap();

            for x in 0..screen.width {
                for y in 0..screen.height {
                    let value = screen.pget(x as u32, y as u32);
                    let rgb_value = PALETTE.lock().unwrap().get_rgb(value);

                    buffer.push(rgb_value.r);
                    buffer.push(rgb_value.g);
                    buffer.push(rgb_value.b);
                }
            }
            self.record.images.append(&mut buffer);
        }

        self.record.nb += 1;*/
    }

    pub fn stop_record(&mut self) {/* 
        info!("[Unicorn] Stop to record the frame {:?}",
              self.record.images.len());

        let screen = &self.screen.lock().unwrap();

        self.record.recording = false;

        let mut filedata = File::create(self.record.filename.clone()).unwrap();

        let mut encoder = gif::Encoder::new(&mut filedata,
                                            screen.width as u16,
                                            screen.height as u16,
                                            &[])
                .unwrap();

        encoder.set(gif::Repeat::Infinite).unwrap();

        let mut idx = 0;
        for i in 0..self.record.images.len() / (screen.width * screen.height * 3) {
            info!("[Unicorn] Generate frame {:?} {:?}/{:?}",
                  i,
                  self.record.images.len(),
                  idx);

            let mut buffer: Vec<u8> = Vec::new();

            for _ in 0..screen.width {
                for _ in 0..screen.height {
                    buffer.push(self.record.images[idx]);
                    buffer.push(self.record.images[idx + 1]);
                    buffer.push(self.record.images[idx + 2]);
                    idx += 3;
                }
            }

            info!("[Unicorn] Creating ImageBuffer {:?}", buffer.len());

            let image =
                image::ImageBuffer::from_raw(screen.height as u32, screen.width as u32, buffer)
                    .unwrap();

            info!("[Unicorn] Rotating image");
            let image = image::DynamicImage::ImageRgb8(image)
                .rotate270()
                .flipv();

            info!("[Unicorn] Creating gif Frame");
            let mut frame = gif::Frame::from_rgb(screen.width as u16,
                                                 screen.height as u16,
                                                 &image.raw_pixels());

            frame.delay = 1;
            encoder.write_frame(&frame).unwrap();
        }

        info!("[Unicorn] GIF created in {:?}", self.record.filename);
        */
    }

    pub fn screenshot(&mut self, filename: &str) {
        /*
        let screen = &mut self.screen.lock().unwrap();

        info!("[Unicorn] Taking screenshot {:?}x{:?} in {:?}", screen.width, screen.height, filename);

        let mut buffer: Vec<u8> = vec![0; (screen.width*screen.height) * 3];

        let mut idx = 0;
        for x in 0..screen.width {
            for y in 0..screen.height {
                let value = screen.pget(x as u32, y as u32);
                let rgb_value = PALETTE.lock().unwrap().get_rgb(value);

                buffer[idx] = rgb_value.r;
                buffer[idx + 1] = rgb_value.g;
                buffer[idx + 2] = rgb_value.b;
                
                idx += 3;
            }
        }

        let image = image::ImageBuffer::from_raw(screen.height as u32, screen.width as u32, buffer)
            .unwrap();
        let image = image::DynamicImage::ImageRgb8(image)
            .rotate270()
            .flipv();

        let mut output = File::create(&Path::new(filename)).unwrap();
        image.save(&mut output, image::ImageFormat::PNG).unwrap();
        */
    }

    pub fn save_current_cartridge(&mut self) {
        /*
        if !self.editing {
            return;
        }

        let screen = &self.screen.lock().unwrap();


        let output_filename = self.cartridge.filename.clone();
        info!("[Unicorn][SAVE] Saving the current cartridge in {:?}",
              output_filename);

        info!("[Unicorn][SAVE] Set the new sprites");
        self.cartridge.gfx.set_sprites(screen.sprites.clone());
        info!("[Unicorn][SAVE] Set the new map");
        self.cartridge.map.set_map(screen.map.clone());
        info!("[Unicorn][SAVE] Set the new flags");
        self.cartridge.gff.set_flags(screen.sprites.clone());
        info!("[Unicorn][SAVE] Set the new palette");
        screen.palette.set_colors(self.palettes.lock().unwrap().get_colors());

        match self.cartridge.format {
            CartridgeFormat::UnicornFormat => {
                cartridge.save_in_unicorn(output_filename,
                                          format!("{:?}.{:?}.{:?}",
                                                self.version,
                                                self.major_version,
                                                self.minor_version)
                                                .as_str());
            }
            _ => {}
        }
        */
    }

    pub fn save_state() {

    }

    pub fn restore_state() {

    }

    pub fn switch_pause(&mut self) {
        info!("[Unicorn] Switch pause");

        let screen = &mut self.screen.lock().unwrap();

        match self.state {
            UnicornState::STOP => {

            }
            UnicornState::PAUSE => {
                self.state = UnicornState::RUN;
                /* Restore previous state */
                screen.restore();
                screen.font(&self.cartridge.font_name.clone());
            }
            UnicornState::RUN => {
                /* Save state */
                screen.save();
                self.cartridge.font_name = screen.get_font();

                screen.font("pico-8");
                self.state = UnicornState::PAUSE;
            }
            UnicornState::EDITOR => {
                /*
                self.pause_menu.reset();
                self.state = UnicornState::PAUSE;
                screen.save();
                */
            }
        }
        info!("[Unicorn] End Switch pause");
    }

    pub fn _setup_screen(&mut self) {
        info!("[Unicorn] Setup screen {:?}", self.cartridge);

        info!("[Unicorn] Copying sprites ...");
        self.screen
            .lock()
            .unwrap()
            .set_sprites(self.cartridge.cartridge.gfx.sprites.clone());

        info!("[Unicorn] Copying gff flags ...");
        self.screen
            .lock()
            .unwrap()
            .set_sprites_flags(self.cartridge.cartridge.gff.flags.clone());

        info!("[Unicorn] Copying map ...");
        self.screen
            .lock()
            .unwrap()
            .set_map(self.cartridge.cartridge.map.map.clone());

        //self.palettes.lock().unwrap().set_colors(cartridge.cartridge.palette.colors.clone());
    }

    pub fn _load_cartridge(&mut self)
                           -> bool {
        info!("[Unicorn] Loading cartridge {:?}", self.cartridge);

        let data = self.cartridge.get_code();

        let mut ret: bool = false;

        match self.cartridge.get_code_type() {
            Code::LUA => {
                info!("[Unicorn] Loading LUA Plugin");

                self.cartridge
                    .lua_plugin
                    .load(self.contexts.clone(),
                          self.info.clone(),
                          self.screen.clone());

                ret = self.cartridge.lua_plugin.load_code(data.clone());
            }
            Code::JAVASCRIPT => {
                info!("[Unicorn] Loading JAVASCRIPT Plugin");

                self.cartridge
                    .javascript_plugin
                    .load(self.contexts.clone(),
                          self.info.clone(),
                          self.screen.clone());

                ret = self.cartridge.javascript_plugin.load_code(data.clone());
            }
            Code::PYTHON => {
                info!("[Unicorn] Loading PYTHON Plugin");

                self.cartridge
                    .python_plugin
                    .load(self.contexts.clone(),
                          self.info.clone(),
                          self.screen.clone());

                ret = self.cartridge.python_plugin.load_code(data.clone());
            }
            _ => (),
        }

        info!("[Unicorn] LOADED CARTRIDGE {:?}", ret);
        ret
    }

    pub fn load_cartridge(&mut self, filename: String) -> bool {
        info!("[Unicorn] Load cartridge from {:?}", filename);

        let cartridge;
        if filename.contains(".corn") {
            match Cartridge::from_unicorn_file(filename.as_str()) {
                Ok(c) => cartridge = c,
                Err(e) => panic!("[Unicorn] Impossible to load the unicorn cartridge {:?}", e),
            }
        } else if filename.contains(".acorn") {
            match Cartridge::from_dunicorn_file(filename.as_str()) {
                Ok(c) => cartridge = c,
                Err(e) => panic!("[Unicorn] Impossible to load the dUnicorn cartridge {:?}", e),
            }
        } else if filename.contains(".png") {
            match Cartridge::from_png_file(filename.as_str()) {
                Ok(c) => cartridge = c,
                Err(e) => panic!("[Unicorn] Impossible to load the Pico8 PNG cartridge {:?}", e),
            }
        } else if filename.contains(".p8") {
            match Cartridge::from_p8_file(filename.as_str()) {
                Ok(c) => cartridge = c,
                Err(e) => panic!("[Unicorn] Impossible to load the Pico8 P8 cartridge {:?}", e),
            }
        } else {
            panic!("[Unicorn] Unknown file format !");
        }

        self.cartridge = UnicornCartridge::new(cartridge, filename);
        self._setup_screen();

        self.cartridge.loaded = self._load_cartridge();
        if self.cartridge.loaded {
            self.state = UnicornState::RUN;
        }

        self.cartridge.loaded
    }

    pub fn switch_code(&mut self) {
        info!("[Unicorn] Switch code");

        #[cfg(feature = "editor")]
        {
            let idx = self.current_cartridge;

            if self.editing {
                info!("[Unicorn] Switch editor to run");

                self.cartridges[idx].set_code(self.editor.get_code());

                // Reload the code for the Unicorn format
               /* match self.cartridges[idx].cartridge.format {
                    CartridgeFormat::UnicornSplittedFormat => {
                        info!("[Unicorn] Reloading code section for the cartridge from the file");
                        self.cartridges[idx].cartridge.code.reload();
                    }
                    CartridgeFormat::UnicornFormat => {
                        info!("[Unicorn] Reloading code section for the cartridge from the buffer");
                        self.cartridges[idx].set_code(self.editor.get_code());
                    }
                    _ => (),
                }*/

                let data = self.cartridges[idx].get_code();
                let code_type = self.cartridges[idx].get_code_type();

                match code_type {
                    Code::LUA => {
                        self.cartridges[idx].lua_plugin.load_code(data);
                    }
                    Code::JAVASCRIPT => {
                        self.cartridges[idx].javascript_plugin.load_code(data);
                    }
                    Code::PYTHON => {
                        self.cartridges[idx].python_plugin.load_code(data);
                    }
                    _ => (),
                }

                self.editing = false;
                self.state = UnicornState::RUN;
                self.reset();
            } else {
                info!("[Unicorn] Switch run to editor");
                info!("[Unicorn] Back to {:?}/{:?}", self.current_cartridge, self.cartridges.len());
                let filename = self.cartridges[self.current_cartridge].filename.clone();
                let full_filename = self.cartridges[self.current_cartridge].full_filename.clone();

                if self.cartridges[self.current_cartridge].loaded == false {
                    self.load_cartridge(filename.as_str(), full_filename.as_str(), false);
                }
                let code = self.cartridges[self.current_cartridge].get_code();

                self.editor
                    .init(self.configuration.clone(),
                          self.palettes.clone(),
                          &mut self.screen.lock().unwrap(),
                          filename,
                          code);
                self.editing = true;
                self.state = UnicornState::EDITOR;
            }

            self.init();
        }
    }

    pub fn call_init(&mut self) {
        info!("[Unicorn] CALL INIT {:?}", self.cartridge.get_code_type());

        match self.cartridge.get_code_type() {
            Code::LUA => match self.cartridge.lua_plugin.init() {
                _ => (),
            }
            Code::JAVASCRIPT => match self.cartridge.javascript_plugin.init() {
                _ => (),
            }
            Code::PYTHON => match self.cartridge.python_plugin.init() {
                _ => (),
            }
            Code::WASM => {}

            _ => error!("[Unicorn] Impossible to match a plugin"),
        }
    }

    pub fn call_draw(&mut self) {
        match self.cartridge.get_code_type()  {
            Code::LUA => {
                match self.cartridge.lua_plugin.draw() {
                    Ok(()) => (),
                    Err(err) => error!("[Unicorn] [call_draw / lua]: {}", err),
                }
            }
            Code::JAVASCRIPT => {
                match self.cartridge.javascript_plugin.draw() {
                    Ok(()) => (),
                    Err(err) => error!("[Unicorn] [call_draw / javascript]: {}", err),
                }
            }
            Code::PYTHON => {
                match self.cartridge.python_plugin.draw() {
                    Ok(()) => (),
                    Err(err) => error!("[Unicorn] [call_draw / python]: {}", err),
                }
            }
            Code::WASM => {

            }

            _ => (),
        }
    }

    pub fn call_update(&mut self) {
        match self.cartridge.get_code_type() {
            Code::LUA => {
                match self.cartridge.lua_plugin.update() {
                    Ok(()) => (),
                    Err(err) => error!("[Unicorn] [call_update / lua]: {}", err),
                }
            }
            
            Code::JAVASCRIPT => {
                match self.cartridge.javascript_plugin.update() {
                    Ok(()) => (),
                    Err(err) => error!("[Unicorn] [call_update / javascript]: {}", err),
                }
            }
            Code::PYTHON => {
                match self.cartridge.python_plugin.update() {
                        Ok(()) => (),
                        Err(err) => error!("[Unicorn] [call_update / python]: {}", err),
                    }
            }
            Code::WASM => {

            }
            _ => (),
        }
    }
}
