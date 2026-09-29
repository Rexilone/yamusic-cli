mod api;
mod app;
mod auth;
mod config;
mod player;
mod ui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::Config;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "yamusic", version, about = "Яндекс Музыка в терминале: Моя волна, лайки, станции")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
    /// Сразу запустить Мою волну
    #[arg(short, long)]
    wave: bool,
    /// Сразу запустить станцию, например genre:rock, mood:calm, activity:workout
    #[arg(short, long, value_name = "ID")]
    station: Option<String>,
    /// Без иконок Nerd Font (если шрифт не установлен)
    #[arg(long)]
    no_icons: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Войти в аккаунт (меню: через браузер или по токену)
    Login {
        /// Использовать готовый OAuth-токен
        #[arg(short, long, conflicts_with = "browser")]
        token: Option<String>,
        /// Сразу войти через окно браузера (токен подхватится автоматически)
        #[arg(short, long)]
        browser: bool,
    },
    /// Выйти (удалить сохранённый токен)
    Logout,
    /// Показать текущий аккаунт
    Whoami,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut cfg = Config::load();

    match cli.command {
        Some(Cmd::Login { token, browser }) => return login(&mut cfg, token, browser),
        Some(Cmd::Logout) => {
            cfg.token = None;
            cfg.save()?;
            println!("Токен удалён.");
            return Ok(());
        }
        Some(Cmd::Whoami) => {
            let mut api = api::Api::new(&token(&mut cfg)?);
            let a = api.account_status()?;
            println!("{} ({}) uid={} Плюс: {}", a.name, a.login, a.uid, if a.has_plus { "да" } else { "нет" });
            return Ok(());
        }
        None => {}
    }

    let token = token(&mut cfg)?;
    let mut api = api::Api::new(&token);
    let account = api.account_status().context("не удалось войти (попробуйте `yamusic login`)")?;

    let (tx, rx) = mpsc::channel();
    let player = player::Player::spawn(tx.clone(), cfg.volume)?;
    let mut app = app::App::new(api, account, tx, player, cfg.volume);
    if let Some(id) = cli.station {
        app.start_station_id(&id);
    } else if cli.wave {
        app.start_station_id(api::MY_WAVE);
    }

    ui::set_nerd_icons(cfg.icons && !cli.no_icons && std::env::var_os("YAMUSIC_NO_ICONS").is_none());
    let mut terminal = ratatui::init();
    // Must run after entering the alternate screen and before reading events.
    app.picker = Some(
        ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::from_fontsize((8, 16))),
    );
    let res = (|| -> Result<()> {
        while !app.quit {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(k) = event::read()? {
                    if k.kind == KeyEventKind::Press {
                        app.on_key(k);
                    }
                }
            }
            while let Ok(m) = rx.try_recv() {
                app.on_msg(m);
            }
        }
        Ok(())
    })();
    ratatui::restore();

    cfg.volume = app.save_volume();
    let _ = cfg.save();
    res
}

fn token(cfg: &mut Config) -> Result<String> {
    if let Some(t) = std::env::var("YAMUSIC_TOKEN").ok().filter(|t| !t.is_empty()) {
        return Ok(t);
    }
    if let Some(t) = cfg.token.clone() {
        return Ok(t);
    }
    println!("Вы не вошли в аккаунт.");
    login(cfg, None, false)?;
    cfg.token.clone().context("нет токена")
}

fn login(cfg: &mut Config, token: Option<String>, browser: bool) -> Result<()> {
    let token = match token {
        Some(t) => auth::parse_token(&t).context("некорректный токен")?,
        None if browser => auth::browser_login()?,
        None => auth::interactive_login()?,
    };
    let mut api = api::Api::new(&token);
    let acc = api.account_status().context("токен не подошёл")?;
    cfg.token = Some(token);
    cfg.save()?;
    println!(
        "\n  ✓ Вход выполнен: {} ({}){}\n  Токен сохранён в {}",
        acc.name,
        acc.login,
        if acc.has_plus { " · Плюс" } else { " · без Плюса (только 30-секундные превью)" },
        config::path().display()
    );
    Ok(())
}
