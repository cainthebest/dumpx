use std::{
    env,
    fs::File,
    io::{self, Read, Write},
    path::PathBuf,
};

const HEADER: &str = "
██████╗ ██╗   ██╗███╗   ███╗██████╗ ██╗  ██╗
██╔══██╗██║   ██║████╗ ████║██╔══██╗╚██╗██╔╝
██║  ██║██║   ██║██╔████╔██║██████╔╝ ╚███╔╝ 
██║  ██║██║   ██║██║╚██╔╝██║██╔═══╝  ██╔██╗ 
██████╔╝╚██████╔╝██║ ╚═╝ ██║██║     ██╔╝ ██╗
╚═════╝  ╚═════╝ ╚═╝     ╚═╝╚═╝     ╚═╝  ╚═╝
";

struct DumpX {
    input: PathBuf,
    output: Option<PathBuf>,
    width: usize,
    group: usize,
    repl: u8,
    hex_region: usize,
}

impl DumpX{
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

    #[inline(always)]
    const fn hi(b: u8) -> u8 {
        Self::HEX_DIGITS[(b >> 4) as usize]
    }

    #[inline(always)]
    const fn lo(b: u8) -> u8 {
        Self::HEX_DIGITS[(b & 0xF) as usize]
    }

    #[inline(always)]
    const fn is_printable(b: u8) -> bool {
        b >= 0x20 && b <= 0x7E
    }

    #[inline(always)]
    const fn calc_hex_region(width: usize, group: usize) -> usize {
        width * 2
            + if group > 0 {
                width.saturating_sub(1) + width / group
            } else {
                width.saturating_sub(1)
            }
    }

    fn new() -> Result<Self, &'static str> {
        let mut args = env::args().skip(1).peekable();

        if args.peek().is_none() {
            Self::help();
        }

        let mut hd = DumpX {
            input: PathBuf::new(),
            output: None,
            width: 16,
            group: 4,
            repl: b'.',
            hex_region: 0,
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-o" | "--output" => {
                    hd.output = Some(PathBuf::from(args.next().ok_or("--output requires file")?))
                }

                "-w" | "--width" => {
                    hd.width = args
                        .next()
                        .ok_or("--width requires number")?
                        .parse()
                        .map_err(|_| "bad width")?
                }

                "-g" | "--group" => {
                    hd.group = args
                        .next()
                        .ok_or("--group requires number")?
                        .parse()
                        .map_err(|_| "bad group")?
                }

                "-r" | "--replacement" => {
                    let s = args.next().ok_or("--replacement requires char")?;
                    let b = s.as_bytes();

                    if b.len() != 1 {
                        return Err("replacement must be single char");
                    }

                    hd.repl = b[0];
                }

                f => {
                    if hd.input.as_os_str().is_empty() {
                        hd.input = PathBuf::from(f);
                    } else {
                        return Err("multiple input files");
                    }
                }
            }
        }

        if hd.input.as_os_str().is_empty() {
            return Err("no input file");
        }

        hd.hex_region = Self::calc_hex_region(hd.width, hd.group);

        Ok(hd)
    }

    #[rustfmt::skip]
    fn help() -> ! {
        // Im not sure on using eprint as i thought to keep help separate from any "real" output

        eprintln!("{}", HEADER);
        eprintln!("                   v{}", Self::VERSION);
        eprintln!("  Command line tool to dump any file as hex\n");
        eprintln!("Usage: dumpx{} [OPTIONS] <INPUT_FILE>\n", env::consts::EXE_SUFFIX);
        eprintln!("  -o, --output <FILE>        write to FILE      [Optional] (stdout default)");
        eprintln!("  -w, --width <NUM>          bytes per line     [Optional] (default 16)");
        eprintln!("  -g, --group <NUM>          group size         [Optional] (default 4)");
        eprintln!("  -r, --replacement <CHAR>   non-printable char [Optional] (default '.')");

        std::process::exit(0)
    }

    fn run(self) -> io::Result<()> {
        let file = File::open(&self.input)?;

        if let Some(ref path) = self.output {
            self.dump(file, File::create(path)?)?;
        } else {
            self.dump(file, io::stdout().lock())?;
        }

        Ok(())
    }

    fn dump<W: Write>(&self, mut file: File, mut out: W) -> io::Result<()> {
        let mut offset = 0usize;
        let mut buf = [0u8; 4096];
        let mut line = Vec::with_capacity(10 + self.hex_region + 1 + self.width + 1);

        while let Ok(n) = file.read(&mut buf) {
            if n == 0 {
                break;
            }

            for chunk in buf[..n].chunks(self.width) {
                line.clear();

                line.extend_from_slice(b"0x");

                for i in (0..8).rev() {
                    line.push(Self::HEX_DIGITS[(offset >> (i * 4)) & 0xF]);
                }

                line.extend_from_slice(b": ");

                for (i, &b) in chunk.iter().enumerate() {
                    if i > 0 {
                        if self.group > 0 && i % self.group == 0 {
                            line.extend_from_slice(b"  ");
                        } else {
                            line.push(b' ');
                        }
                    }

                    line.push(Self::hi(b));
                    line.push(Self::lo(b));
                }

                if line.len() - 10 < self.hex_region {
                    line.resize(10 + self.hex_region, b' ');
                }

                line.extend_from_slice(b"      ");

                for &b in chunk {
                    line.push(if Self::is_printable(b) { b } else { self.repl });
                }

                line.push(b'\n');
                out.write_all(&line)?;
                offset += chunk.len();
            }
        }

        Ok(())
    }
}

fn main() {
    match DumpX::new() {
        Err(e) => {
            eprintln!("Error: {}", e);

            std::process::exit(1);
        }

        Ok(d) => {
            if let Err(e) = d.run() {
                eprintln!("Error: {}", e);

                std::process::exit(1);
            }
        }
    }
}
