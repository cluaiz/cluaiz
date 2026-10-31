//! The Cluaiz mascot: its text art (the agent's intro uses it), and a small playground opened by
//! `cluaiz play` or `/play`: animated backdrops, a mascot that hops when clicked, and a runner game.
//! The caller owns the screen (raw mode, alternate screen, mouse capture); `play` only draws into it.

use std::{
    io::{self, Stdout, Write},
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    cursor::{Hide, MoveTo},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};

/// The mascot, 15 rows of `░▒▓` text
pub const ART: &str = "              ░▒▓▓▓▓▓▓▓▓▓▒░\n            ▒▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▒\n         ▒▓▓▒░              ░▒▓░\n        ▓▓                     ▒▒\n       ▓▓     ░▓▓░      ▓▓░     ▒▒\n      ░▓▓      ▓▓       ▒▓      ▒▒░\n      ▓▓▓         ▒▓▓▓▒         ▒▒▒\n     ░▓▓▓▓          ░          ▒▒▒▒░\n     ▓▓▓▓▓▒                   ▒▓▒▒▒▒\n   ░▓▓▓▓▓▓▓▓▒░            ░░▒▒▒▒▒▒▒░\n  ▓▓▓▓▓▓▓▓▓▓▓▓▓▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒\n    ░░░░ ▒▒▒▒▒▒▒▒▒▒▒░▒▒▒▒▒▒▒▒▒░  ░░░\n          ▓▒▒▒▒▒▒░░   ░▒▒▒▒▒▒▒▒\n          ▒▓▒▒▒▒░       ▒▒▒▒▒▒░\n            ░░░          ░░░░";
/// The same mascot with its eyes closed (same rows and widths)
pub const ART_BLINK: &str = "              ░▒▓▓▓▓▓▓▓▓▓▒░\n            ▒▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▒\n         ▒▓▓▒░              ░▒▓░\n        ▓▓                     ▒▒\n       ▓▓     ───       ───     ▒▒\n      ░▓▓                       ▒▒░\n      ▓▓▓         ▒▓▓▓▒         ▒▒▒\n     ░▓▓▓▓          ░          ▒▒▒▒░\n     ▓▓▓▓▓▒                   ▒▓▒▒▒▒\n   ░▓▓▓▓▓▓▓▓▒░            ░░▒▒▒▒▒▒▒░\n  ▓▓▓▓▓▓▓▓▓▓▓▓▓▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒\n    ░░░░ ▒▒▒▒▒▒▒▒▒▒▒░▒▒▒▒▒▒▒▒▒░  ░░░\n          ▓▒▒▒▒▒▒░░   ░▒▒▒▒▒▒▒▒\n          ▒▓▒▒▒▒░       ▒▒▒▒▒▒░\n            ░░░          ░░░░";
/// Smallest window the playground draws in
pub const MIN_SIZE: (u16, u16) = (79, 25);
const ROWS: i32 = 15;

#[derive(Clone, Copy, PartialEq)]
struct Pixel {
    ch: char,
    color: Color,
}
impl Default for Pixel {
    fn default() -> Self { Self { ch: ' ', color: Color::White } }
}

struct Canvas {
    width: u16,
    height: u16,
    cells: Vec<Pixel>,
}
impl Canvas {
    fn new(width: u16, height: u16) -> Self {
        Self { width, height, cells: vec![Pixel::default(); width as usize * height as usize] }
    }
    fn put(&mut self, x: i32, y: i32, ch: char, color: Color) {
        if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
            self.cells[y as usize * self.width as usize + x as usize] = Pixel { ch, color };
        }
    }
    fn text(&mut self, x: i32, y: i32, value: &str, color: Color) {
        for (n, ch) in value.chars().enumerate() { self.put(x + n as i32, y, ch, color); }
    }
    fn flush_changes(&self, previous: &[Pixel], out: &mut Stdout) -> io::Result<()> {
        for (index, &cell) in self.cells.iter().enumerate() {
            if previous.get(index) == Some(&cell) { continue; }
            let x = (index % self.width as usize) as u16;
            let y = (index / self.width as usize) as u16;
            queue!(out, MoveTo(x, y), SetForegroundColor(cell.color), Print(cell.ch))?;
        }
        queue!(out, ResetColor)?;
        out.flush()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode { Ambient, Runner }
#[derive(Clone, Copy)]
enum Backdrop { Stars, Rain, Matrix }

struct Obstacle { x: f32, height: i32 }
struct Particle { x: f32, y: f32, vx: f32, vy: f32, life: f32 }

struct Game {
    mode: Mode,
    backdrop: Backdrop,
    elapsed: f32,
    jump: f32,
    velocity: f32,
    paused: bool,
    dead: bool,
    score: f32,
    best: u32,
    next_obstacle: f32,
    obstacles: Vec<Obstacle>,
    particles: Vec<Particle>,
    blink_until: f32,
    click_counter: u32,
}
impl Game {
    fn new() -> Self {
        Self {
            mode: Mode::Ambient, backdrop: Backdrop::Stars, elapsed: 0.0, jump: 0.0, velocity: 0.0,
            paused: false, dead: false, score: 0.0, best: 0,
            next_obstacle: 2.2, obstacles: vec![], particles: vec![],
            blink_until: 0.0, click_counter: 0,
        }
    }
    fn set_mode(&mut self, mode: Mode) {
        let best = self.best;
        let backdrop = self.backdrop;
        *self = Self::new();
        self.mode = mode;
        self.backdrop = backdrop;
        self.best = best;
    }
    fn cycle_backdrop(&mut self) {
        self.backdrop = match self.backdrop {
            Backdrop::Stars => Backdrop::Rain,
            Backdrop::Rain => Backdrop::Matrix,
            Backdrop::Matrix => Backdrop::Stars,
        };
    }
    fn jump(&mut self, width: u16, height: u16) {
        if self.dead { self.set_mode(Mode::Runner); return; }
        if self.paused { return; }
        if self.jump <= 0.01 { self.velocity = 19.0; }
        self.emit_sparkles(width, height);
    }
    fn emit_sparkles(&mut self, width: u16, height: u16) {
        self.click_counter = self.click_counter.wrapping_add(1);
        let (x, y) = mascot_position(self, width, height);
        for i in 0..14u32 {
            let angle = ((i * 97 + self.click_counter * 31) % 360) as f32 * std::f32::consts::PI / 180.0;
            let speed = 3.0 + (i % 5) as f32;
            self.particles.push(Particle {
                x: (x + 18) as f32, y: (y + 7) as f32,
                vx: angle.cos() * speed * 1.8, vy: angle.sin() * speed,
                life: 0.45 + 0.05 * (i % 4) as f32,
            });
        }
    }
    fn tick(&mut self, dt: f32, width: u16, height: u16) {
        if self.paused { return; }
        self.elapsed += dt;
        if self.jump > 0.0 || self.velocity > 0.0 {
            self.jump += self.velocity * dt;
            self.velocity -= 44.0 * dt;
            if self.jump <= 0.0 { self.jump = 0.0; self.velocity = 0.0; }
        }
        self.particles.retain_mut(|p| {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy += dt * 12.0;
            p.life -= dt;
            p.life > 0.0
        });
        if self.mode != Mode::Runner || self.dead { return; }
        self.score += dt * 10.0;
        self.best = self.best.max(self.score as u32);
        self.next_obstacle -= dt;
        if self.next_obstacle <= 0.0 {
            let height_cells = if (self.score as u32 / 23) % 3 == 0 { 3 } else { 2 };
            self.obstacles.push(Obstacle { x: width as f32 - 3.0, height: height_cells });
            self.next_obstacle = 1.8 + 0.3 * ((self.score as i32 / 10) % 3) as f32;
        }
        let speed = (14.0 + self.score / 60.0).min(23.0);
        for obstacle in &mut self.obstacles { obstacle.x -= speed * dt; }
        self.obstacles.retain(|o| o.x >= -4.0);
        let (sx, _) = mascot_position(self, width, height);
        // Hitbox is on the mascot's visible lower torso/paws (not its transparent margins).
        let left = sx as f32 + 8.0;
        let right = sx as f32 + 30.0;
        if self.obstacles.iter().any(|o| o.x < right && o.x + 3.0 > left && self.jump < o.height as f32 + 0.3) {
            self.dead = true;
        }
    }
}

fn hash64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn mascot_position(game: &Game, width: u16, height: u16) -> (i32, i32) {
    let x = (width as i32 / 13).max(2);
    let ground = height as i32 - 5;
    let idle_bob = if game.mode == Mode::Ambient && (game.elapsed * 2.0).sin() > 0.6 { 1 } else { 0 };
    (x, ground - ROWS - game.jump.round() as i32 - idle_bob)
}

fn draw_mascot(canvas: &mut Canvas, x: i32, y: i32, blink: bool) {
    for (row, line) in if blink { ART_BLINK } else { ART }.lines().enumerate() {
        let characters: Vec<char> = line.chars().collect();
        let first = characters.iter().position(|&ch| ch != ' ');
        let last = characters.iter().rposition(|&ch| ch != ' ');
        if let (Some(first), Some(last)) = (first, last) {
            // Mask the background behind the character's body AND dark face.
            // This prevents stars from showing through the face/body.
            for column in first..=last {
                let ch = characters[column];
                let color = match ch {
                    '▓' => Color::Rgb { r: 249, g: 250, b: 253 },
                    '▒' => Color::Rgb { r: 192, g: 202, b: 218 },
                    '░' => Color::Rgb { r: 126, g: 148, b: 169 },
                    '─' => Color::Rgb { r: 206, g: 221, b: 234 },
                    _ => Color::Black,
                };
                canvas.put(x + column as i32, y + row as i32, ch, color);
            }
        }
    }
}

fn draw(canvas: &mut Canvas, game: &Game) {
    let width = canvas.width as i32;
    let height = canvas.height as i32;
    if width < MIN_SIZE.0 as i32 || height < MIN_SIZE.1 as i32 {
        canvas.text(2, 2, "Cluaiz Mascot needs 79+ columns and 25+ rows.", Color::Yellow);
        canvas.text(2, 4, "Resize terminal or reduce terminal font size. [Q] quit", Color::Grey);
        return;
    }
    let green = Color::Rgb { r: 165, g: 218, b: 125 };
    let muted = Color::Rgb { r: 97, g: 116, b: 133 };
    let dim = Color::Rgb { r: 65, g: 94, b: 108 };
    let white = Color::Rgb { r: 224, g: 233, b: 239 };
    let ground = height - 5;

    // Three subtle terminal-native backdrops, completely behind the mascot layer.
    // 3 cycles: parallax stars -> rain -> falling digital glyphs.
    let points = ((width * (height - 5)) / 110).clamp(10, 140);
    for i in 0..points {
        let seed = hash64((i as u64 + 1) * 1277);
        let depth = (i % 3 + 1) as f32;
        let origin_x = (seed % width as u64) as i32;
        let origin_y = 4 + ((seed >> 19) % (height - 10) as u64) as i32;
        match game.backdrop {
            Backdrop::Stars => {
                let shift = (game.elapsed * depth * 1.2) as i32;
                let x = (origin_x - shift).rem_euclid(width);
                let c = if (game.elapsed * 3.0) as i32 % 6 == i % 6 { '+' } else { '·' };
                canvas.put(x, origin_y, c, if i % 7 == 0 { muted } else { dim });
            }
            Backdrop::Rain => {
                let y = 4 + (origin_y - 4 + (game.elapsed * (8.0 + depth * 3.0)) as i32).rem_euclid(height - 10);
                canvas.put(origin_x, y, '│', dim);
                canvas.put(origin_x, y + 1, '·', muted);
            }
            Backdrop::Matrix => {
                let x = origin_x;
                let y = 4 + (origin_y - 4 + (game.elapsed * (3.0 + depth)) as i32).rem_euclid(height - 10);
                canvas.put(x, y, if i % 2 == 0 { '0' } else { '1' }, if i % 5 == 0 { green } else { dim });
                if y > 5 { canvas.put(x, y - 1, '·', dim); }
            }
        }
    }

    canvas.text(2, 0, "CLUAIZ  /  MASCOT PLAYGROUND", green);
    canvas.text(2, 2, "[1] AMBIENT   [2] RUNNER   [3] BACKDROP   [SPACE/CLICK] HOP   [B] BLINK   [P] PAUSE   [R] RESET   [Q] EXIT", muted);
    canvas.text(2, 3, &"─".repeat((width as usize).saturating_sub(4)), dim);

    // Draw simple ground; obstacle and character are layered above it.
    for x in 0..width { canvas.put(x, ground, if x % 6 == 0 { '┄' } else { '─' }, dim); }
    let scroll = (game.elapsed * 12.0) as i32;
    for i in 0..width / 8 {
        let x = (i * 8 - scroll).rem_euclid(width);
        canvas.put(x, ground + 1, '·', dim);
    }
    for obstacle in &game.obstacles {
        let x = obstacle.x.round() as i32;
        for column in 0..3 {
            for row in 0..obstacle.height {
                canvas.put(x + column, ground - row - 1, '▓', if row == obstacle.height - 1 { green } else { muted });
            }
        }
    }

    let (avatar_x, avatar_y) = mascot_position(game, canvas.width, canvas.height);
    let blink = game.elapsed < game.blink_until || (game.elapsed % 4.7) > 4.56;
    draw_mascot(canvas, avatar_x, avatar_y, blink);
    for p in &game.particles {
        canvas.put(p.x.round() as i32, p.y.round() as i32, if p.life > 0.25 { '*' } else { '·' }, green);
    }

    let mode_label = if game.mode == Mode::Ambient { "AMBIENT - click to bounce" } else { "DINO RUN - jump over obstacles" };
    canvas.text(2, ground + 2, mode_label, white);
    canvas.text(2, ground + 3, &format!("SCORE {:05}    BEST {:05}", game.score as u32, game.best), green);
    if game.dead {
        canvas.text(width / 2 - 16, height / 2, "GAME OVER  -  R or CLICK to retry", Color::Yellow);
    } else if game.paused {
        canvas.text(width / 2 - 9, height / 2, "PAUSED  [P] RESUME", Color::Yellow);
    }
}

/// Runs the playground until Q, Esc or Ctrl+C. The caller redraws its own screen afterwards.
pub fn play(out: &mut Stdout) -> io::Result<()> {
    execute!(out, Hide, Clear(ClearType::All))?;
    let mut game = Game::new();
    let mut previous = Vec::<Pixel>::new();
    let mut previous_size = (0u16, 0u16);
    let mut last = Instant::now();
    let target = Duration::from_millis(33); // 30 FPS cap, even while idle.

    loop {
        let frame_started = Instant::now();
        let dt = frame_started.duration_since(last).as_secs_f32().min(0.05);
        last = frame_started;
        let (width, height) = terminal::size()?;
        if (width, height) != previous_size {
            previous.clear();
            previous_size = (width, height);
            queue!(out, Clear(ClearType::All))?;
        }
        // Limit events per frame so continuous mouse-move input cannot starve rendering.
        for _ in 0..64 {
            if !event::poll(Duration::ZERO)? { break; }
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press && key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') => return Ok(()),
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('1') => game.set_mode(Mode::Ambient),
                    KeyCode::Char('2') => game.set_mode(Mode::Runner),
                    KeyCode::Char('3') => game.cycle_backdrop(),
                    KeyCode::Char('p') => game.paused = !game.paused,
                    KeyCode::Char('r') => { let m = game.mode; game.set_mode(m); }
                    KeyCode::Char('b') => game.blink_until = game.elapsed + 0.17,
                    KeyCode::Char(' ') | KeyCode::Up => game.jump(width, height),
                    _ => {}
                },
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if game.mode == Mode::Ambient {
                            let (x, y) = mascot_position(&game, width, height);
                            let cx = mouse.column as i32;
                            let cy = mouse.row as i32;
                            if (x + 5..=x + 40).contains(&cx) && (y..y + ROWS).contains(&cy) {
                                // Pet the mascot: bounce, blink and a little sparkle burst.
                                game.jump(width, height);
                                game.blink_until = game.elapsed + 0.16;
                            } else {
                                // A background click starts the Dino-style runner.
                                game.set_mode(Mode::Runner);
                                game.jump(width, height);
                            }
                        } else {
                            game.jump(width, height);
                        }
                    },
                    MouseEventKind::Down(MouseButton::Right) => {
                        game.set_mode(if game.mode == Mode::Ambient { Mode::Runner } else { Mode::Ambient });
                    },
                    _ => {}
                },
                _ => {}
            }
        }
        game.tick(dt, width, height);
        let mut canvas = Canvas::new(width, height);
        draw(&mut canvas, &game);
        canvas.flush_changes(&previous, out)?;
        previous = canvas.cells;
        let frame_spent = frame_started.elapsed();
        if frame_spent < target { thread::sleep(target - frame_spent); }
    }
}
