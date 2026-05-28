mod app;
mod cli;
mod events;
mod logger;

fn main() {
    let args = cli::Cli::parse_from_env();
    println!("{:?}", args);
}
