use base64::{engine::general_purpose, Engine as _};
use clap::{Parser, ValueEnum};
use publicsuffix::Host::{Domain, Ip};
use std::{fs::File, io::Write};

#[derive(Debug, Copy, Clone, PartialEq, ValueEnum)]
enum Mode {
    SmartDNS,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // Dns Group Name
    #[arg(short, long)]
    group: String,
    // Dns Mode, Only Support SmartDNS
    #[arg(short, long)]
    mode: Mode,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let url = "https://raw.githubusercontent.com/gfwlist/gfwlist/master/gfwlist.txt";
    let body = reqwest::get(url).await?.text().await?;
    let decoded = body
        .lines()
        .map(|line| general_purpose::STANDARD.decode(line))
        .flat_map(Result::unwrap)
        .collect();
    let decoded = String::from_utf8(decoded).unwrap();
    let list = publicsuffix::List::fetch().unwrap();

    let (lines, errors): (Vec<_>, Vec<_>) = decoded
        .lines()
        .map(str::trim)
        .filter(|line| {
            !(line.starts_with('!')
                || line.starts_with('@')
                || line.starts_with('[')
                || line.contains(".*")
                || line.is_empty())
        })
        .map(transform_with(list))
        .partition(Result::is_ok);

    let mut lines: Vec<_> = lines.into_iter().map(Result::unwrap).collect();
    let before_dedup = lines.len();
    lines.sort();
    lines.dedup();

    lines = lines
        .into_iter()
        .map(|line| format!("nameserver /{}/{}", line, args.group))
        .collect();

    let mut output = File::create("gfwlist.domain.smartdns.conf")?;
    output.write_all(lines.join("\n").as_bytes())?;

    let errors: Vec<_> = errors.into_iter().map(Result::unwrap_err).collect();
    eprintln!(
        "Total {} lines (already removed {} duplicated) transformed.",
        lines.len(),
        before_dedup - lines.len()
    );
    eprintln!("{} lines can't transform.", errors.len());

    Ok(())
}

fn transform_with(list: publicsuffix::List) -> impl Fn(&str) -> Result<String, String> {
    move |line| {
        let mut line = String::from(line);
        ["||", "|", "http://", "https://", "*", "."]
            .iter()
            .for_each(|search| {
                if line.starts_with(search) {
                    line = line.replacen(search, "", 1);
                }
            });
        line = line.replace('*', "/");

        match list.parse_url(&(String::from("http://") + &line)) {
            Ok(host) => {
                let name = host.to_string();
                match host {
                    Domain(ref domain) if domain.has_known_suffix() => Ok(name),
                    Domain(_) => Err(name + " is invalid."),
                    Ip(_) => Err(name + " is a IP."),
                }
            }
            Err(error) => Err(error.to_string() + " in " + &line),
        }
    }
}
