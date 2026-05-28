mod cli;
mod events;

fn main() {
    let args = cli::Cli::parse_from_env();
    println!("{:?}", args);
}
