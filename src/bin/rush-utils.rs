use std::ffi::OsString;

fn main() {
    let mut args = std::env::args_os();
    let _binary = args.next();
    let Some(utility) = args.next() else {
        eprintln!("rush-utils: expected a utility name");
        std::process::exit(2);
    };
    let Some(name) = utility.to_str() else {
        eprintln!("rush-utils: utility name is not valid UTF-8");
        std::process::exit(2);
    };
    let utility_args = std::iter::once(OsString::from(name)).chain(args);
    let status = match name {
        "cat" => uu_cat::uumain(utility_args),
        "cp" => uu_cp::uumain(utility_args),
        "echo" => uu_echo::uumain(utility_args),
        "ls" => uu_ls::uumain(utility_args),
        "mkdir" => uu_mkdir::uumain(utility_args),
        "mv" => uu_mv::uumain(utility_args),
        "pwd" => uu_pwd::uumain(utility_args),
        "rm" => uu_rm::uumain(utility_args),
        "sort" => uu_sort::uumain(utility_args),
        "touch" => uu_touch::uumain(utility_args),
        "uniq" => uu_uniq::uumain(utility_args),
        "unzip" => rush::archive::unzip_main(utility_args),
        "wc" => uu_wc::uumain(utility_args),
        "zip" => rush::archive::zip_main(utility_args),
        _ => {
            eprintln!("rush-utils: {name}: utility is not bundled");
            127
        }
    };
    std::process::exit(status);
}
