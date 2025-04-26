//! DumpX: A simple hexdump utility tool.
//!
//! This reads a file and writes its contents in hexadecimal and ASCII,
//! grouping bytes per line and replacing non-ASCII bytes with a dot.
//!
//! # Usage
//!
//! ```text
//! dumpx <INPUT_FILE_PATH> [OPTIONS]
//!
//! Options:
//!   -o, --output <OUTPUT_FILE_PATH>    Write to a new file (default: stdout)
//! ```

use std::{
    env,
    fs::File,
    io::{self, Read, Write},
    path::PathBuf,
    process,
};

struct DumpX {
    /// Path to the input file to read and dump.
    input: PathBuf,

    /// Optional path to the output file. If `None`, writes to stdout.
    output: Option<PathBuf>,
}

impl DumpX {
    /// Header with version and usage instructions.
    const HEADER: &'static str = concat!(
        "██████╗ ██╗   ██╗███╗   ███╗██████╗ ██╗  ██╗\n",
        "██╔══██╗██║   ██║████╗ ████║██╔══██╗╚██╗██╔╝\n",
        "██║  ██║██║   ██║██╔████╔██║██████╔╝ ╚███╔╝ \n",
        "██║  ██║██║   ██║██║╚██╔╝██║██╔═══╝  ██╔██╗ \n",
        "██████╔╝╚██████╔╝██║ ╚═╝ ██║██║     ██╔╝ ██╗\n",
        "╚═════╝  ╚═════╝ ╚═╝     ╚═╝╚═╝     ╚═╝  ╚═╝\n",
        "                   v",
        env!("CARGO_PKG_VERSION"),
        "                    ",
        "\n",
        "Usage: dumpx <INPUT_FILE_PATH> [OPTIONS]",
        "\n\n",
        "Options:",
        "\n",
        "  -o, --output <OUTPUT_FILE_PATH>  Write to a new file  [Optional]  (Default: stdout)",
        "\n",
    );

    /// Number of bytes per output line.
    const WIDTH: usize = 16;

    /// Number of bytes per grouping within each line.
    const GROUP_SIZE: usize = 4;

    /// Placeholder byte for non-printable ASCII characters.
    const NON_ASCII: u8 = b'.';

    /// Length of the offset prefix in the output line.
    ///
    /// "0x" + 8 hex digits + ": "
    const OFFSET_LEN: usize = 2 + 8 + 2;

    /// Length of the hex section in the output line.
    const HEX_SECTION: usize =
        Self::WIDTH * 2 + (Self::WIDTH - 1) + (Self::WIDTH / Self::GROUP_SIZE - 1);

    /// Length of the ASCII section in the output line.
    ///
    /// "  " + WIDTH chars + newline
    const ASCII_SECTION: usize = 2 + Self::WIDTH + 1;

    /// Total buffer size needed per line: offset + hex section + ASCII section.
    const LINE_BUF_SIZE: usize = Self::OFFSET_LEN + Self::HEX_SECTION + Self::ASCII_SECTION;
    /// I/O buffer size for reading chunks from the file.
    const IO_BUF_SIZE: usize = 64 * 1024;

    /// Lookup table for converting a 4 bit value to its hex ASCII representation.
    const NIBBLE_LUT: [u8; 16] = *b"0123456789abcdef";

    /// Precomputed lookup for each byte to its two character hex representation.
    const HEX_LUT: [[u8; 2]; 256] = {
        let mut m = [[b'0'; 2]; 256];
        let mut i = 0;

        while i < 256 {
            m[i] = [Self::NIBBLE_LUT[i >> 4], Self::NIBBLE_LUT[i & 0xF]];
            i += 1;
        }
        m
    };

    /// Parses command line arguments to construct a `DumpX` instance.
    ///
    /// On no arguments, prints the header and exits successfully.
    ///
    /// Returns an error string if parsing fails.
    fn new() -> Result<Self, &'static str> {
        let mut args = env::args().skip(1).peekable();

        let mut input = PathBuf::new();
        let mut output = None;

        // If no args provided, show usage header and exit
        if args.peek().is_none() {
            print!("{}", Self::HEADER);

            process::exit(0);
        }

        // Iterate through arguments.
        while let Some(arg) = args.next() {
            match arg.as_str() {
                // Handle output flag and its value
                "-o" | "--output" => {
                    output = Some(PathBuf::from(args.next().ok_or("--output requires file")?));
                }

                // First non flag is the input file path
                f => {
                    if input.as_os_str().is_empty() {
                        input = PathBuf::from(f);
                    } else {
                        // More than one input arg specified
                        return Err("multiple input files");
                    }
                }
            }
        }

        // Ensure at least one input file was provided
        if input.as_os_str().is_empty() {
            return Err("missing input file");
        }

        Ok(DumpX { input, output })
    }

    /// Opens the input file and dispatches to `dump`, handling output location.
    fn run(self) -> io::Result<()> {
        let file = File::open(&self.input)?;

        if let Some(ref path) = self.output {
            // Prevent overwriting existing files
            if path.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("output file '{}' already exists", path.display()),
                ));
            }

            // Create a new output file and perform the dump
            self.dump(file, File::create(path)?)?;
        } else {
            // No output file: write to stdout
            self.dump(file, io::stdout().lock())?;
        }

        Ok(())
    }

    /// Reads the input file in chunks and writes formatted lines to `out`.
    fn dump<W: Write>(&self, mut file: File, mut out: W) -> io::Result<()> {
        let mut io_buf = [0u8; Self::IO_BUF_SIZE];

        let mut line_offset = 0usize;
        let mut line_buf = [0u8; Self::LINE_BUF_SIZE];

        // Read the file until EOF
        while let Ok(n) = file.read(&mut io_buf) {
            if n == 0 {
                break;
            }

            // Process each WIDTH sized chunk from the buffer
            for chunk in io_buf[..n].chunks(Self::WIDTH) {
                let mut i = 0;

                // Prefix section: Write the offset prefix, e.g. "0x00000000: "
                //TODO: Handle 4GiB+ offsets
                //TODO: ngl thats quite a bit of data to look at but its possible someone might
                //TODO: If we use 16 digits after 4GiB, then we can support 16EiB

                line_buf[i..i + 2].copy_from_slice(b"0x");
                i += 2;

                for shift in (0..8).rev() {
                    line_buf[i] = Self::NIBBLE_LUT[(line_offset >> (shift * 4)) & 0xF];
                    i += 1;
                }

                line_buf[i..i + 2].copy_from_slice(b": ");
                i += 2;

                // Hex section: group bytes and insert spaces

                let mut hex_written = 0;
                for (j, &b) in chunk.iter().enumerate() {
                    if j > 0 {
                        if j % Self::GROUP_SIZE == 0 {
                            line_buf[i..i + 2].copy_from_slice(b"  ");
                            i += 2;
                            hex_written += 2;
                        } else {
                            line_buf[i] = b' ';
                            i += 1;
                            hex_written += 1;
                        }
                    }

                    // Copy the 2 char hex for this byte
                    line_buf[i..i + 2].copy_from_slice(&Self::HEX_LUT[b as usize]);
                    i += 2;
                    hex_written += 2;
                }

                // Pad any remaining space in the hex section
                for _ in 0..(Self::HEX_SECTION - hex_written) {
                    line_buf[i] = b' ';
                    i += 1;
                }

                // Separator between hex and ASCII sections
                line_buf[i..i + 2].copy_from_slice(b"  ");
                i += 2;

                // ASCII section: printable bytes or placeholder

                for &b in chunk.iter() {
                    line_buf[i] = if (0x20..=0x7E).contains(&b) {
                        b
                    } else {
                        Self::NON_ASCII
                    };
                    i += 1;
                }

                // Add newline
                line_buf[i] = b'\n';
                i += 1;

                // Write the completed line to output
                out.write_all(&line_buf[..i])?;

                // Update the offset for the next line
                line_offset += chunk.len();
            }
        }

        Ok(())
    }
}

fn main() {
    match DumpX::new() {
        Err(e) => {
            eprintln!("Error: {}", e);

            process::exit(1);
        }

        Ok(d) => {
            if let Err(e) = d.run() {
                eprintln!("Error: {}", e);

                process::exit(1);
            }
        }
    }
}
