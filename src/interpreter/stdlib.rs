use std::collections::HashMap;
use std::path::Path;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use std::io::Read;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}, OnceLock};
use std::io::Write;

use crate::interpreter::json::{json_parse, json_pretty, json_stringify, json_validate};
use crate::interpreter::value::Value;
use super::gc::{GcData, GcHeap};
use regex::Regex;

type FnMap = HashMap<String, Value>;

macro_rules! builtin {
    ($name:expr) => {
        Value::BuiltinFn(concat!("stdlib.", $name).to_string())
    };
}

macro_rules! num {
    ($n:expr) => {
        Value::Float($n)
    };
}

pub fn get_all() -> HashMap<String, Value> {
    let mut modules: HashMap<String, Value> = HashMap::new();
    modules.insert("math".into(), Value::Module(math_fns()));
    modules.insert("fs".into(), Value::Module(fs_fns()));
    modules.insert("os".into(), Value::Module(os_fns()));
    modules.insert("datetime".into(), Value::Module(datetime_fns()));
    modules.insert("json".into(), Value::Module(json_fns()));
    modules.insert("str".into(), Value::Module(str_fns()));
    modules.insert("list".into(), Value::Module(list_fns()));
    modules.insert("net".into(), Value::Module(net_fns()));
    modules.insert("random".into(), Value::Module(random_fns()));
    modules.insert("encoding".into(), Value::Module(encoding_fns()));
    modules.insert("set".into(), Value::Module(set_fns()));
    modules.insert("regex".into(), Value::Module(regex_fns()));
    modules.insert("process".into(), Value::Module(process_fns()));
    modules.insert("hashlib".into(), Value::Module(hashlib_fns()));
    modules.insert("path".into(), Value::Module(path_fns()));
    modules.insert("csv".into(), Value::Module(csv_fns()));
    modules.insert("logging".into(), Value::Module(logging_fns()));
    modules.insert("threading".into(), Value::Module(threading_fns()));
    modules
}

fn math_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("pi".into(), num!(std::f64::consts::PI));
    m.insert("e".into(), num!(std::f64::consts::E));
    m.insert("tau".into(), num!(std::f64::consts::TAU));
    m.insert("inf".into(), num!(f64::INFINITY));
    m.insert("nan".into(), num!(f64::NAN));
    m.insert("sin".into(), builtin!("math.sin"));
    m.insert("cos".into(), builtin!("math.cos"));
    m.insert("tan".into(), builtin!("math.tan"));
    m.insert("asin".into(), builtin!("math.asin"));
    m.insert("acos".into(), builtin!("math.acos"));
    m.insert("atan".into(), builtin!("math.atan"));
    m.insert("atan2".into(), builtin!("math.atan2"));
    m.insert("sqrt".into(), builtin!("math.sqrt"));
    m.insert("pow".into(), builtin!("math.pow"));
    m.insert("log".into(), builtin!("math.log"));
    m.insert("log2".into(), builtin!("math.log2"));
    m.insert("log10".into(), builtin!("math.log10"));
    m.insert("exp".into(), builtin!("math.exp"));
    m.insert("abs".into(), builtin!("math.abs"));
    m.insert("floor".into(), builtin!("math.floor"));
    m.insert("ceil".into(), builtin!("math.ceil"));
    m.insert("round".into(), builtin!("math.round"));
    m.insert("sign".into(), builtin!("math.sign"));
    m.insert("max".into(), builtin!("math.max"));
    m.insert("min".into(), builtin!("math.min"));
    m.insert("clamp".into(), builtin!("math.clamp"));
    m.insert("lerp".into(), builtin!("math.lerp"));
    m.insert("deg_to_rad".into(), builtin!("math.deg_to_rad"));
    m.insert("rad_to_deg".into(), builtin!("math.rad_to_deg"));
    m
}

fn fs_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("read".into(), builtin!("fs.read"));
    m.insert("write".into(), builtin!("fs.write"));
    m.insert("append".into(), builtin!("fs.append"));
    m.insert("exists".into(), builtin!("fs.exists"));
    m.insert("is_file".into(), builtin!("fs.is_file"));
    m.insert("is_dir".into(), builtin!("fs.is_dir"));
    m.insert("create_dir".into(), builtin!("fs.create_dir"));
    m.insert("create_dir_all".into(), builtin!("fs.create_dir_all"));
    m.insert("remove".into(), builtin!("fs.remove"));
    m.insert("rename".into(), builtin!("fs.rename"));
    m.insert("copy".into(), builtin!("fs.copy"));
    m.insert("list_dir".into(), builtin!("fs.list_dir"));
    m.insert("size".into(), builtin!("fs.size"));
    m.insert("metadata".into(), builtin!("fs.metadata"));
    m
}

fn os_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("name".into(), builtin!("os.name"));
    m.insert("arch".into(), builtin!("os.arch"));
    m.insert("family".into(), builtin!("os.family"));
    m.insert("args".into(), builtin!("os.args"));
    m.insert("get_env".into(), builtin!("os.get_env"));
    m.insert("set_env".into(), builtin!("os.set_env"));
    m.insert("remove_env".into(), builtin!("os.remove_env"));
    m.insert("home_dir".into(), builtin!("os.home_dir"));
    m.insert("current_dir".into(), builtin!("os.current_dir"));
    m.insert("set_current_dir".into(), builtin!("os.set_current_dir"));
    m.insert("pid".into(), builtin!("os.pid"));
    m.insert("host_name".into(), builtin!("os.host_name"));
    m.insert("temp_dir".into(), builtin!("os.temp_dir"));
    m.insert("executable_path".into(), builtin!("os.executable_path"));
    m.insert("linesep".into(), Value::String("\n".into()));
    m
}

fn datetime_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("now".into(), builtin!("datetime.now"));
    m.insert("timestamp".into(), builtin!("datetime.timestamp"));
    m.insert("year".into(), builtin!("datetime.year"));
    m.insert("month".into(), builtin!("datetime.month"));
    m.insert("day".into(), builtin!("datetime.day"));
    m.insert("hour".into(), builtin!("datetime.hour"));
    m.insert("minute".into(), builtin!("datetime.minute"));
    m.insert("second".into(), builtin!("datetime.second"));
    m.insert("from_timestamp".into(), builtin!("datetime.from_timestamp"));
    m.insert("sleep_ms".into(), builtin!("datetime.sleep_ms"));
    m.insert("format".into(), builtin!("datetime.format"));
    m
}

fn json_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("encode".into(), builtin!("json.encode"));
    m.insert("decode".into(), builtin!("json.decode"));
    m.insert("pretty".into(), builtin!("json.pretty"));
    m.insert("validate".into(), builtin!("json.validate"));
    m.insert("encode_file".into(), builtin!("json.encode_file"));
    m.insert("decode_file".into(), builtin!("json.decode_file"));
    m
}

fn str_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("trim".into(), builtin!("str.trim"));
    m.insert("upper".into(), builtin!("str.upper"));
    m.insert("lower".into(), builtin!("str.lower"));
    m.insert("contains".into(), builtin!("str.contains"));
    m.insert("replace".into(), builtin!("str.replace"));
    m.insert("split".into(), builtin!("str.split"));
    m.insert("starts_with".into(), builtin!("str.starts_with"));
    m.insert("ends_with".into(), builtin!("str.ends_with"));
    m.insert("repeat".into(), builtin!("str.repeat"));
    m.insert("reverse".into(), builtin!("str.reverse"));
    m.insert("pad_start".into(), builtin!("str.pad_start"));
    m.insert("pad_end".into(), builtin!("str.pad_end"));
    m.insert("bytes".into(), builtin!("str.bytes"));
    m.insert("is_digit".into(), builtin!("str.is_digit"));
    m.insert("is_alpha".into(), builtin!("str.is_alpha"));
    m.insert("is_alphanum".into(), builtin!("str.is_alphanum"));
    m.insert("is_lower".into(), builtin!("str.is_lower"));
    m.insert("is_upper".into(), builtin!("str.is_upper"));
    m.insert("capitalize".into(), builtin!("str.capitalize"));
    m.insert("title".into(), builtin!("str.title"));
    m.insert("find".into(), builtin!("str.find"));
    m.insert("count".into(), builtin!("str.count"));
    m.insert("strip".into(), builtin!("str.strip"));
    m.insert("ljust".into(), builtin!("str.ljust"));
    m.insert("rjust".into(), builtin!("str.rjust"));
    m.insert("center".into(), builtin!("str.center"));
    m.insert("zfill".into(), builtin!("str.zfill"));
    m
}

fn list_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("push".into(), builtin!("list.push"));
    m.insert("pop".into(), builtin!("list.pop"));
    m.insert("sort".into(), builtin!("list.sort"));
    m.insert("reverse".into(), builtin!("list.reverse"));
    m.insert("join".into(), builtin!("list.join"));
    m.insert("map".into(), builtin!("list.map"));
    m.insert("filter".into(), builtin!("list.filter"));
    m.insert("fold".into(), builtin!("list.fold"));
    m.insert("take".into(), builtin!("list.take"));
    m.insert("collect".into(), builtin!("list.collect"));
    m.insert("flatten".into(), builtin!("list.flatten"));
    m.insert("unique".into(), builtin!("list.unique"));
    m.insert("chunk".into(), builtin!("list.chunk"));
    m.insert("fill".into(), builtin!("list.fill"));
    m.insert("insert".into(), builtin!("list.insert"));
    m.insert("remove".into(), builtin!("list.remove"));
    m.insert("index".into(), builtin!("list.index"));
    m.insert("count".into(), builtin!("list.count"));
    m.insert("extend".into(), builtin!("list.extend"));
    m
}

fn net_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("http_get".into(), builtin!("net.http_get"));
    m.insert("http_post".into(), builtin!("net.http_post"));
    m.insert("http_put".into(), builtin!("net.http_put"));
    m.insert("http_delete".into(), builtin!("net.http_delete"));
    m.insert("fetch".into(), builtin!("net.fetch"));
    m.insert("tcp_connect".into(), builtin!("net.tcp_connect"));
    m.insert("dns_lookup".into(), builtin!("net.dns_lookup"));
    m
}

fn random_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("int".into(), builtin!("random.int"));
    m.insert("float".into(), builtin!("random.float"));
    m.insert("seed".into(), builtin!("random.seed"));
    m.insert("choice".into(), builtin!("random.choice"));
    m.insert("shuffle".into(), builtin!("random.shuffle"));
    m.insert("uuid".into(), builtin!("random.uuid"));
    m.insert("normal".into(), builtin!("random.normal"));
    m.insert("bytes".into(), builtin!("random.bytes"));
    m
}

fn encoding_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("base64_encode".into(), builtin!("encoding.base64_encode"));
    m.insert("base64_decode".into(), builtin!("encoding.base64_decode"));
    m.insert("hex_encode".into(), builtin!("encoding.hex_encode"));
    m.insert("hex_decode".into(), builtin!("encoding.hex_decode"));
    m
}

fn regex_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("is_match".into(), builtin!("regex.is_match"));
    m.insert("find".into(), builtin!("regex.find"));
    m.insert("find_all".into(), builtin!("regex.find_all"));
    m.insert("replace".into(), builtin!("regex.replace"));
    m.insert("split".into(), builtin!("regex.split"));
    m
}

fn process_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("run".into(), builtin!("process.run"));
    m.insert("output".into(), builtin!("process.output"));
    m.insert("spawn".into(), builtin!("process.spawn"));
    m
}

fn hashlib_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("md5".into(), builtin!("hashlib.md5"));
    m.insert("sha1".into(), builtin!("hashlib.sha1"));
    m.insert("sha256".into(), builtin!("hashlib.sha256"));
    m.insert("sha512".into(), builtin!("hashlib.sha512"));
    m
}

fn path_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("join".into(), builtin!("path.join"));
    m.insert("dirname".into(), builtin!("path.dirname"));
    m.insert("basename".into(), builtin!("path.basename"));
    m.insert("extension".into(), builtin!("path.extension"));
    m.insert("stem".into(), builtin!("path.stem"));
    m.insert("parent".into(), builtin!("path.parent"));
    m.insert("split".into(), builtin!("path.split"));
    m.insert("resolve".into(), builtin!("path.resolve"));
    m.insert("relative".into(), builtin!("path.relative"));
    m.insert("normalize".into(), builtin!("path.normalize"));
    m.insert("is_absolute".into(), builtin!("path.is_absolute"));
    m.insert("separator".into(), Value::String(std::path::MAIN_SEPARATOR.to_string()));
    m
}

fn csv_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("parse".into(), builtin!("csv.parse"));
    m.insert("encode".into(), builtin!("csv.encode"));
    m.insert("parse_file".into(), builtin!("csv.parse_file"));
    m.insert("encode_file".into(), builtin!("csv.encode_file"));
    m
}

fn logging_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("set_level".into(), builtin!("logging.set_level"));
    m.insert("debug".into(), builtin!("logging.debug"));
    m.insert("info".into(), builtin!("logging.info"));
    m.insert("warn".into(), builtin!("logging.warn"));
    m.insert("error".into(), builtin!("logging.error"));
    m.insert("fatal".into(), builtin!("logging.fatal"));
    m.insert("set_file".into(), builtin!("logging.set_file"));
    m.insert("set_format".into(), builtin!("logging.set_format"));
    m
}

fn threading_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("mutex".into(), builtin!("threading.mutex"));
    m.insert("lock".into(), builtin!("threading.lock"));
    m.insert("try_lock".into(), builtin!("threading.try_lock"));
    m.insert("unlock".into(), builtin!("threading.unlock"));
    m.insert("semaphore".into(), builtin!("threading.semaphore"));
    m.insert("acquire".into(), builtin!("threading.acquire"));
    m.insert("try_acquire".into(), builtin!("threading.try_acquire"));
    m.insert("release".into(), builtin!("threading.release"));
    m.insert("rwlock".into(), builtin!("threading.rwlock"));
    m.insert("read_lock".into(), builtin!("threading.read_lock"));
    m.insert("read_unlock".into(), builtin!("threading.read_unlock"));
    m.insert("write_lock".into(), builtin!("threading.write_lock"));
    m.insert("write_unlock".into(), builtin!("threading.write_unlock"));
    m.insert("synchronized".into(), builtin!("threading.synchronized"));
    m
}

fn set_fns() -> FnMap {
    let mut m = FnMap::new();
    m.insert("add".into(), builtin!("set.add"));
    m.insert("remove".into(), builtin!("set.remove"));
    m.insert("contains".into(), builtin!("set.contains"));
    m.insert("union".into(), builtin!("set.union"));
    m.insert("intersection".into(), builtin!("set.intersection"));
    m.insert("difference".into(), builtin!("set.difference"));
    m.insert("is_subset".into(), builtin!("set.is_subset"));
    m.insert("to_list".into(), builtin!("set.to_list"));
    m
}

fn get_string(v: &Value) -> Result<&str, String> {
    match v {
        Value::String(s) => Ok(s.as_str()),
        v => Err(format!("Expected string, got {}", v.type_name())),
    }
}

fn get_number(v: &Value) -> Result<f64, String> {
    v.as_float().ok_or_else(|| format!("Expected number, got {}", v.type_name()))
}

pub fn call_stdlib(module: &str, name: &str, _args: &[Value], gc: &mut GcHeap) -> Result<Value, String> {
    match (module, name) {
        // ==================== MATH ====================
        ("math", "sin") => one_num(_args, f64::sin),
        ("math", "cos") => one_num(_args, f64::cos),
        ("math", "tan") => one_num(_args, f64::tan),
        ("math", "asin") => one_num(_args, f64::asin),
        ("math", "acos") => one_num(_args, f64::acos),
        ("math", "atan") => one_num(_args, f64::atan),
        ("math", "atan2") => two_num(_args, f64::atan2),
        ("math", "sqrt") => one_num(_args, f64::sqrt),
        ("math", "log") => one_num(_args, f64::ln),
        ("math", "log2") => one_num(_args, f64::log2),
        ("math", "log10") => one_num(_args, f64::log10),
        ("math", "exp") => one_num(_args, f64::exp),
        ("math", "abs") => one_num(_args, f64::abs),
        ("math", "floor") => one_num(_args, f64::floor),
        ("math", "ceil") => one_num(_args, f64::ceil),
        ("math", "round") => one_num(_args, f64::round),
        ("math", "sign") => one_num(_args, |n| if n > 0.0 { 1.0 } else if n < 0.0 { -1.0 } else { 0.0 }),
        ("math", "max") => two_num(_args, |a, b| a.max(b)),
        ("math", "min") => two_num(_args, |a, b| a.min(b)),
        ("math", "pow") => two_num(_args, |a, b| a.powf(b)),
        ("math", "clamp") => {
            if _args.len() != 3 { return Err("clamp() expects 3 args".into()); }
            let v = get_number(&_args[0])?;
            let lo = get_number(&_args[1])?;
            let hi = get_number(&_args[2])?;
            Ok(num!(v.clamp(lo, hi)))
        }
        ("math", "lerp") => {
            if _args.len() != 3 { return Err("lerp() expects 3 args".into()); }
            let a = get_number(&_args[0])?;
            let b = get_number(&_args[1])?;
            let t = get_number(&_args[2])?;
            Ok(num!(a + (b - a) * t))
        }
        ("math", "deg_to_rad") => one_num(_args, |d| d.to_radians()),
        ("math", "rad_to_deg") => one_num(_args, |r| r.to_degrees()),

        // ==================== FS ====================
        ("fs", "read") => {
            if _args.len() != 1 { return Err("fs.read() expects 1 arg".into()); }
            let path = get_string(&_args[0])?.to_string();
            fs::read_to_string(Path::new(&path))
                .map(Value::String)
                .map_err(|e| format!("Could not read '{}': {}", path, e))
        }
        ("fs", "write") => {
            if _args.len() != 2 { return Err("fs.write() expects 2 args".into()); }
            let path = get_string(&_args[0])?.to_string();
            let content = get_string(&_args[1])?.to_string();
            fs::write(Path::new(&path), &content)
                .map_err(|e| format!("Could not write '{}': {}", path, e))?;
            Ok(Value::Nil)
        }
        ("fs", "append") => {
            if _args.len() != 2 { return Err("fs.append() expects 2 args".into()); }
            let path = get_string(&_args[0])?.to_string();
            let content = get_string(&_args[1])?.to_string();
            let mut file = fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(Path::new(&path))
                .map_err(|e| format!("Could not open '{}': {}", path, e))?;
            use std::io::Write;
            file.write_all(content.as_bytes())
                .map_err(|e| format!("Could not append to '{}': {}", path, e))?;
            Ok(Value::Nil)
        }
        ("fs", "exists") => {
            if _args.len() != 1 { return Err("fs.exists() expects 1 arg".into()); }
            Ok(Value::Boolean(Path::new(get_string(&_args[0])?).exists()))
        }
        ("fs", "is_file") => {
            if _args.len() != 1 { return Err("fs.is_file() expects 1 arg".into()); }
            Ok(Value::Boolean(Path::new(get_string(&_args[0])?).is_file()))
        }
        ("fs", "is_dir") => {
            if _args.len() != 1 { return Err("fs.is_dir() expects 1 arg".into()); }
            Ok(Value::Boolean(Path::new(get_string(&_args[0])?).is_dir()))
        }
        ("fs", "create_dir") => {
            if _args.len() != 1 { return Err("fs.create_dir() expects 1 arg".into()); }
            let path = get_string(&_args[0])?;
            fs::create_dir(Path::new(path))
                .map_err(|e| format!("Could not create dir '{}': {}", path, e))?;
            Ok(Value::Nil)
        }
        ("fs", "create_dir_all") => {
            if _args.len() != 1 { return Err("fs.create_dir_all() expects 1 arg".into()); }
            let path = get_string(&_args[0])?;
            fs::create_dir_all(Path::new(path))
                .map_err(|e| format!("Could not create dirs '{}': {}", path, e))?;
            Ok(Value::Nil)
        }
        ("fs", "remove") => {
            if _args.len() != 1 { return Err("fs.remove() expects 1 arg".into()); }
            let path = get_string(&_args[0])?;
            fs::remove_file(Path::new(path))
                .or_else(|_| fs::remove_dir_all(Path::new(path)))
                .map_err(|e| format!("Could not remove '{}': {}", path, e))?;
            Ok(Value::Nil)
        }
        ("fs", "rename") => {
            if _args.len() != 2 { return Err("fs.rename() expects 2 args".into()); }
            let from = get_string(&_args[0])?;
            let to = get_string(&_args[1])?;
            fs::rename(Path::new(from), Path::new(to))
                .map_err(|e| format!("Could not rename '{}' to '{}': {}", from, to, e))?;
            Ok(Value::Nil)
        }
        ("fs", "copy") => {
            if _args.len() != 2 { return Err("fs.copy() expects 2 args".into()); }
            let from = get_string(&_args[0])?;
            let to = get_string(&_args[1])?;
            fs::copy(Path::new(from), Path::new(to))
                .map_err(|e| format!("Could not copy '{}' to '{}': {}", from, to, e))?;
            Ok(Value::Nil)
        }
        ("fs", "list_dir") => {
            if _args.len() != 1 { return Err("fs.list_dir() expects 1 arg".into()); }
            let path = get_string(&_args[0])?;
            let entries = fs::read_dir(Path::new(path))
                .map_err(|e| format!("Could not list '{}': {}", path, e))?;
            let mut items = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name().to_string_lossy().to_string();
                items.push(Value::String(name));
            }
            Ok(Value::List(gc.alloc(GcData::List(items))))
        }
        ("fs", "size") => {
            if _args.len() != 1 { return Err("fs.size() expects 1 arg".into()); }
            let path = get_string(&_args[0])?;
            let meta = fs::metadata(Path::new(path))
                .map_err(|e| format!("Could not stat '{}': {}", path, e))?;
            Ok(Value::Int(meta.len() as i64))
        }
        ("fs", "metadata") => {
            if _args.len() != 1 { return Err("fs.metadata() expects 1 arg".into()); }
            let path = get_string(&_args[0])?;
            let meta = fs::metadata(Path::new(path))
                .map_err(|e| format!("Could not stat '{}': {}", path, e))?;
            let mut dict = Vec::new();
            dict.push((Value::String("size".into()), Value::Int(meta.len() as i64)));
            dict.push((Value::String("is_file".into()), Value::Boolean(meta.is_file())));
            dict.push((Value::String("is_dir".into()), Value::Boolean(meta.is_dir())));
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                dict.push((Value::String("permissions".into()), Value::Int(meta.mode() as i64)));
            }
            Ok(Value::Dict(gc.alloc(GcData::Dict(dict))))
        }

        // ==================== OS ====================
        ("os", "name") => {
            Ok(Value::String(std::env::consts::OS.to_string()))
        }
        ("os", "arch") => {
            Ok(Value::String(std::env::consts::ARCH.to_string()))
        }
        ("os", "family") => {
            Ok(Value::String(std::env::consts::FAMILY.to_string()))
        }
        ("os", "args") => {
            let args: Vec<Value> = std::env::args().map(Value::String).collect();
            Ok(Value::List(gc.alloc(GcData::List(args))))
        }
        ("os", "get_env") => {
            if _args.len() != 1 { return Err("os.get_env() expects 1 arg".into()); }
            let name = get_string(&_args[0])?;
            match std::env::var(name) {
                Ok(v) => Ok(Value::String(v)),
                Err(_) => Ok(Value::Nil),
            }
        }
        ("os", "set_env") => {
            if _args.len() != 2 { return Err("os.set_env() expects 2 args".into()); }
            let name = get_string(&_args[0])?.to_string();
            let value = get_string(&_args[1])?.to_string();
            unsafe { std::env::set_var(name, value); }
            Ok(Value::Nil)
        }
        ("os", "remove_env") => {
            if _args.len() != 1 { return Err("os.remove_env() expects 1 arg".into()); }
            let name = get_string(&_args[0])?;
            unsafe { std::env::remove_var(name); }
            Ok(Value::Nil)
        }
        ("os", "home_dir") => {
            let dir = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map_err(|_| "Could not determine home directory".to_string())?;
            Ok(Value::String(dir))
        }
        ("os", "current_dir") => {
            std::env::current_dir()
                .map(|p| Value::String(p.to_string_lossy().to_string()))
                .map_err(|e| e.to_string())
        }
        ("os", "set_current_dir") => {
            if _args.len() != 1 { return Err("os.set_current_dir() expects 1 arg".into()); }
            let path = get_string(&_args[0])?;
            std::env::set_current_dir(Path::new(path))
                .map_err(|e| format!("Could not change dir to '{}': {}", path, e))?;
            Ok(Value::Nil)
        }
        ("os", "pid") => {
            Ok(Value::Int(std::process::id() as i64))
        }
        ("os", "host_name") => {
            let name = std::fs::read_to_string("/proc/sys/kernel/hostname")
                .or_else(|_| std::env::var("HOSTNAME"))
                .map_err(|_| "Could not determine hostname".to_string())?;
            Ok(Value::String(name.trim().to_string()))
        }
        ("os", "temp_dir") => {
            Ok(Value::String(std::env::temp_dir().to_string_lossy().to_string()))
        }
        ("os", "executable_path") => {
            std::env::current_exe()
                .map(|p| Value::String(p.to_string_lossy().to_string()))
                .map_err(|e| e.to_string())
        }

        // ==================== DATETIME ====================
        ("datetime", "timestamp") => {
            let dur = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?;
            Ok(Value::Float(dur.as_secs_f64()))
        }
        ("datetime", "now") => {
            let ts = unix_timestamp()?;
            let (y, mo, d, h, mi, s) = unix_ts_to_components(ts);
            Ok(Value::String(format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, mi, s)))
        }
        ("datetime", "year") => {
            let ts = unix_timestamp()?;
            Ok(num!(unix_ts_to_components(ts).0 as f64))
        }
        ("datetime", "month") => {
            let ts = unix_timestamp()?;
            Ok(num!(unix_ts_to_components(ts).1 as f64))
        }
        ("datetime", "day") => {
            let ts = unix_timestamp()?;
            Ok(num!(unix_ts_to_components(ts).2 as f64))
        }
        ("datetime", "hour") => {
            let ts = unix_timestamp()?;
            Ok(num!(unix_ts_to_components(ts).3 as f64))
        }
        ("datetime", "minute") => {
            let ts = unix_timestamp()?;
            Ok(num!(unix_ts_to_components(ts).4 as f64))
        }
        ("datetime", "second") => {
            let ts = unix_timestamp()?;
            Ok(num!(unix_ts_to_components(ts).5 as f64))
        }
        ("datetime", "from_timestamp") => {
            if _args.len() != 1 { return Err("datetime.from_timestamp() expects 1 arg".into()); }
            let ts = get_number(&_args[0])? as i64;
            let (y, mo, d, h, mi, s) = unix_ts_to_components(ts);
            Ok(Value::String(format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, mi, s)))
        }
        ("datetime", "sleep_ms") => {
            if _args.len() != 1 { return Err("datetime.sleep_ms() expects 1 arg".into()); }
            let ms = get_number(&_args[0])? as u64;
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Ok(Value::Nil)
        }
        ("datetime", "format") => {
            if _args.len() != 1 { return Err("datetime.format() expects 1 arg".into()); }
            let fmt = get_string(&_args[0])?;
            let ts = unix_timestamp()?;
            let (y, mo, d, h, mi, s) = unix_ts_to_components(ts);
            let mut result = String::new();
            let mut chars = fmt.chars();
            while let Some(c) = chars.next() {
                if c == '%' {
                    match chars.next() {
                        Some('Y') => result.push_str(&format!("{:04}", y)),
                        Some('m') => result.push_str(&format!("{:02}", mo)),
                        Some('d') => result.push_str(&format!("{:02}", d)),
                        Some('H') => result.push_str(&format!("{:02}", h)),
                        Some('M') => result.push_str(&format!("{:02}", mi)),
                        Some('S') => result.push_str(&format!("{:02}", s)),
                        Some('s') => result.push_str(&ts.to_string()),
                        Some('c') => result.push_str(&format!("{}", ts)),
                        Some('%') => result.push('%'),
                        Some(other) => result.push(other),
                        None => result.push('%'),
                    }
                } else {
                    result.push(c);
                }
            }
            Ok(Value::String(result))
        }

        // ==================== JSON ====================
        ("json", "encode") => {
            if _args.len() != 1 { return Err("json.encode() expects 1 arg".into()); }
            Ok(Value::String(json_stringify(&_args[0], gc)))
        }
        ("json", "decode") => {
            if _args.len() != 1 { return Err("json.decode() expects 1 arg".into()); }
            let s = get_string(&_args[0])?.to_string();
            json_parse(&s, gc)
        }
        ("json", "pretty") => {
            if _args.len() != 1 { return Err("json.pretty() expects 1 arg".into()); }
            Ok(Value::String(json_pretty(&_args[0], 0, gc)))
        }
        ("json", "validate") => {
            if _args.len() != 1 { return Err("json.validate() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            Ok(Value::Boolean(json_validate(s, gc)))
        }
        ("json", "encode_file") => {
            if _args.len() != 2 { return Err("json.encode_file() expects 2 args".into()); }
            let path = get_string(&_args[0])?.to_string();
            let content = json_stringify(&_args[1], gc);
            fs::write(Path::new(&path), &content)
                .map_err(|e| format!("Could not write '{}': {}", path, e))?;
            Ok(Value::Nil)
        }
        ("json", "decode_file") => {
            if _args.len() != 1 { return Err("json.decode_file() expects 1 arg".into()); }
            let path = get_string(&_args[0])?.to_string();
            let content = fs::read_to_string(Path::new(&path))
                .map_err(|e| format!("Could not read '{}': {}", path, e))?;
            json_parse(&content, gc)
        }

        // ==================== STR ====================
        ("str", "trim") => {
            if _args.len() != 1 { return Err("str.trim() expects 1 arg".into()); }
            Ok(Value::String(get_string(&_args[0])?.trim().to_string()))
        }
        ("str", "upper") => {
            if _args.len() != 1 { return Err("str.upper() expects 1 arg".into()); }
            Ok(Value::String(get_string(&_args[0])?.to_uppercase()))
        }
        ("str", "lower") => {
            if _args.len() != 1 { return Err("str.lower() expects 1 arg".into()); }
            Ok(Value::String(get_string(&_args[0])?.to_lowercase()))
        }
        ("str", "contains") => {
            if _args.len() != 2 { return Err("str.contains() expects 2 args".into()); }
            Ok(Value::Boolean(get_string(&_args[0])?.contains(get_string(&_args[1])?)))
        }
        ("str", "replace") => {
            if _args.len() != 3 { return Err("str.replace() expects 3 args".into()); }
            let s = get_string(&_args[0])?;
            let from = get_string(&_args[1])?;
            let to = get_string(&_args[2])?;
            Ok(Value::String(s.replace(from, to)))
        }
        ("str", "split") => {
            if _args.len() != 2 { return Err("str.split() expects 2 args".into()); }
            let s = get_string(&_args[0])?.to_string();
            let sep = get_string(&_args[1])?;
            let parts: Vec<Value> = s.split(sep).map(|p| Value::String(p.into())).collect();
            Ok(Value::List(gc.alloc(GcData::List(parts))))
        }
        ("str", "starts_with") => {
            if _args.len() != 2 { return Err("str.starts_with() expects 2 args".into()); }
            Ok(Value::Boolean(get_string(&_args[0])?.starts_with(get_string(&_args[1])?)))
        }
        ("str", "ends_with") => {
            if _args.len() != 2 { return Err("str.ends_with() expects 2 args".into()); }
            Ok(Value::Boolean(get_string(&_args[0])?.ends_with(get_string(&_args[1])?)))
        }
        ("str", "repeat") => {
            if _args.len() != 2 { return Err("str.repeat() expects 2 args".into()); }
            let s = get_string(&_args[0])?;
            let n = get_number(&_args[1])? as usize;
            Ok(Value::String(s.repeat(n)))
        }
        ("str", "reverse") => {
            if _args.len() != 1 { return Err("str.reverse() expects 1 arg".into()); }
            Ok(Value::String(get_string(&_args[0])?.chars().rev().collect()))
        }
        ("str", "pad_start") => {
            if _args.len() != 3 { return Err("str.pad_start() expects 3 args".into()); }
            let s = get_string(&_args[0])?;
            let len = get_number(&_args[1])? as usize;
            let pad = get_string(&_args[2])?;
            if let Some(pad_char) = pad.chars().next() {
                Ok(Value::String(format!("{}{}", pad_char.to_string().repeat(len.saturating_sub(s.len())), s)))
            } else {
                Ok(Value::String(s.to_string()))
            }
        }
        ("str", "pad_end") => {
            if _args.len() != 3 { return Err("str.pad_end() expects 3 args".into()); }
            let s = get_string(&_args[0])?;
            let len = get_number(&_args[1])? as usize;
            let pad = get_string(&_args[2])?;
            if let Some(pad_char) = pad.chars().next() {
                Ok(Value::String(format!("{}{}", s, pad_char.to_string().repeat(len.saturating_sub(s.len())))))
            } else {
                Ok(Value::String(s.to_string()))
            }
        }
        ("str", "bytes") => {
            if _args.len() != 1 { return Err("str.bytes() expects 1 arg".into()); }
            Ok(Value::Int(get_string(&_args[0])?.len() as i64))
        }
        ("str", "is_digit") => {
            if _args.len() != 1 { return Err("str.is_digit() expects 1 arg".into()); }
            Ok(Value::Boolean(get_string(&_args[0])?.chars().all(|c| c.is_ascii_digit())))
        }
        ("str", "is_alpha") => {
            if _args.len() != 1 { return Err("str.is_alpha() expects 1 arg".into()); }
            Ok(Value::Boolean(get_string(&_args[0])?.chars().all(|c| c.is_ascii_alphabetic())))
        }
        ("str", "is_alphanum") => {
            if _args.len() != 1 { return Err("str.is_alphanum() expects 1 arg".into()); }
            Ok(Value::Boolean(get_string(&_args[0])?.chars().all(|c| c.is_ascii_alphanumeric())))
        }
        ("str", "is_lower") => {
            if _args.len() != 1 { return Err("str.is_lower() expects 1 arg".into()); }
            Ok(Value::Boolean(get_string(&_args[0])?.chars().all(|c| !c.is_ascii_uppercase())))
        }
        ("str", "is_upper") => {
            if _args.len() != 1 { return Err("str.is_upper() expects 1 arg".into()); }
            Ok(Value::Boolean(get_string(&_args[0])?.chars().all(|c| !c.is_ascii_lowercase())))
        }
        ("str", "capitalize") => {
            if _args.len() != 1 { return Err("str.capitalize() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            let mut chars = s.chars();
            let capitalized = match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            };
            Ok(Value::String(capitalized))
        }
        ("str", "title") => {
            if _args.len() != 1 { return Err("str.title() expects 1 arg".into()); }
            let mut result = String::new();
            let mut prev_was_space = true;
            for c in get_string(&_args[0])?.chars() {
                if prev_was_space {
                    result.extend(c.to_uppercase());
                } else {
                    result.push(c);
                }
                prev_was_space = c.is_whitespace();
            }
            Ok(Value::String(result))
        }
        ("str", "find") => {
            if _args.len() != 2 { return Err("str.find() expects 2 args".into()); }
            let s = get_string(&_args[0])?;
            let sub = get_string(&_args[1])?;
            match s.find(sub) {
                Some(i) => Ok(Value::Int(i as i64)),
                None => Ok(Value::Int(-1)),
            }
        }
        ("str", "count") => {
            if _args.len() != 2 { return Err("str.count() expects 2 args".into()); }
            let s = get_string(&_args[0])?;
            let sub = get_string(&_args[1])?;
            if sub.is_empty() { return Ok(Value::Int(0)); }
            let mut count = 0;
            let mut start = 0;
            while let Some(i) = s[start..].find(sub) {
                count += 1;
                start += i + sub.len();
            }
            Ok(Value::Int(count))
        }
        ("str", "strip") => {
            if _args.len() != 1 { return Err("str.strip() expects 1 arg".into()); }
            Ok(Value::String(get_string(&_args[0])?.trim().to_string()))
        }
        ("str", "ljust") => {
            if _args.len() < 2 || _args.len() > 3 { return Err("str.ljust() expects 2 or 3 args".into()); }
            let s = get_string(&_args[0])?;
            let width = get_number(&_args[1])? as usize;
            if s.len() >= width { return Ok(Value::String(s.to_string())); }
            let pad_char = if _args.len() == 3 { get_string(&_args[2])?.chars().next().unwrap_or(' ') } else { ' ' };
            Ok(Value::String(format!("{}{}", s, pad_char.to_string().repeat(width - s.len()))))
        }
        ("str", "rjust") => {
            if _args.len() < 2 || _args.len() > 3 { return Err("str.rjust() expects 2 or 3 args".into()); }
            let s = get_string(&_args[0])?;
            let width = get_number(&_args[1])? as usize;
            if s.len() >= width { return Ok(Value::String(s.to_string())); }
            let pad_char = if _args.len() == 3 { get_string(&_args[2])?.chars().next().unwrap_or(' ') } else { ' ' };
            Ok(Value::String(format!("{}{}", pad_char.to_string().repeat(width - s.len()), s)))
        }
        ("str", "center") => {
            if _args.len() < 2 || _args.len() > 3 { return Err("str.center() expects 2 or 3 args".into()); }
            let s = get_string(&_args[0])?;
            let width = get_number(&_args[1])? as usize;
            if s.len() >= width { return Ok(Value::String(s.to_string())); }
            let pad_char = if _args.len() == 3 { get_string(&_args[2])?.chars().next().unwrap_or(' ') } else { ' ' };
            let left = (width - s.len()) / 2;
            let right = width - s.len() - left;
            Ok(Value::String(format!("{}{}{}", pad_char.to_string().repeat(left), s, pad_char.to_string().repeat(right))))
        }
        ("str", "zfill") => {
            if _args.len() != 2 { return Err("str.zfill() expects 2 args".into()); }
            let s = get_string(&_args[0])?;
            let width = get_number(&_args[1])? as usize;
            if s.len() >= width { return Ok(Value::String(s.to_string())); }
            let zeros = "0".repeat(width - s.len());
            Ok(Value::String(zeros + s))
        }

        // ==================== LIST ====================
        ("list", "push") => {
            if _args.len() != 2 { return Err("list.push() expects 2 args".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get_mut(*h) {
                        GcData::List(vec) => {
                            vec.push(_args[1].clone());
                            Ok(Value::List(*h))
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.push() expects list, got {}", v.type_name())),
            }
        }
        ("list", "pop") => {
            if _args.len() != 1 { return Err("list.pop() expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get_mut(*h) {
                        GcData::List(vec) => vec.pop().ok_or_else(|| "pop() on empty list".into()),
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.pop() expects list, got {}", v.type_name())),
            }
        }
        ("list", "sort") => {
            if _args.len() != 1 { return Err("list.sort() expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get_mut(*h) {
                        GcData::List(vec) => {
                            vec.sort_by_key(|a| a.to_string());
                            Ok(Value::List(*h))
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.sort() expects list, got {}", v.type_name())),
            }
        }
        ("list", "reverse") => {
            if _args.len() != 1 { return Err("list.reverse() expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get_mut(*h) {
                        GcData::List(vec) => {
                            vec.reverse();
                            Ok(Value::List(*h))
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.reverse() expects list, got {}", v.type_name())),
            }
        }
        ("list", "join") => {
            if _args.len() != 2 { return Err("list.join() expects 2 args".into()); }
            let sep = get_string(&_args[1])?;
            match &_args[0] {
                Value::List(h) => {
                    match gc.get(*h) {
                        GcData::List(vec) => {
                            let strs: Vec<String> = vec.iter().map(|v| v.to_string()).collect();
                            Ok(Value::String(strs.join(sep)))
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.join() expects list, got {}", v.type_name())),
            }
        }
        ("list", "slice") => {
            if _args.len() < 2 || _args.len() > 3 { return Err("list.slice() expects 2 or 3 args".into()); }
            match &_args[0] {
                Value::List(h) => {
                    let start = get_number(&_args[1])? as usize;
                    let vec = match gc.get(*h) { GcData::List(v) => v.clone(), _ => unreachable!() };
                    let end = if _args.len() == 3 {
                        get_number(&_args[2])? as usize
                    } else {
                        vec.len()
                    };
                    if start > vec.len() || end > vec.len() || start > end {
                        return Err("Invalid slice bounds".into());
                    }
                    Ok(Value::List(gc.alloc(GcData::List(vec[start..end].to_vec()))))
                }
                v => Err(format!("list.slice() expects list, got {}", v.type_name())),
            }
        }
        ("list", "fill") => {
            if _args.len() != 2 { return Err("list.fill() expects 2 args".into()); }
            let n = get_number(&_args[0])? as usize;
            Ok(Value::List(gc.alloc(GcData::List(vec![_args[1].clone(); n]))))
        }
        ("list", "unique") => {
            if _args.len() != 1 { return Err("list.unique() expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    let vec = match gc.get(*h) { GcData::List(v) => v.clone(), _ => unreachable!() };
                    let mut seen = Vec::new();
                    let mut result = Vec::new();
                    for item in vec.iter() {
                        if !seen.contains(item) {
                            seen.push(item.clone());
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::List(gc.alloc(GcData::List(result))))
                }
                v => Err(format!("list.unique() expects list, got {}", v.type_name())),
            }
        }
        ("list", "flatten") => {
            if _args.len() != 1 { return Err("list.flatten() expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    let vec = match gc.get(*h) { GcData::List(v) => v.clone(), _ => unreachable!() };
                    let mut result: Vec<Value> = Vec::new();
                    for item in vec.iter() {
                        match item {
                            Value::List(inner_h) => {
                                let inner = match gc.get(*inner_h) { GcData::List(v) => v.clone(), _ => unreachable!() };
                                result.extend(inner);
                            }
                            other => result.push(other.clone()),
                        }
                    }
                    Ok(Value::List(gc.alloc(GcData::List(result))))
                }
                v => Err(format!("list.flatten() expects list, got {}", v.type_name())),
            }
        }
        ("list", "chunk") => {
            if _args.len() != 2 { return Err("list.chunk() expects 2 args".into()); }
            let size = get_number(&_args[1])? as usize;
            if size == 0 { return Err("chunk size must be > 0".into()); }
            match &_args[0] {
                Value::List(h) => {
                    let vec = match gc.get(*h) { GcData::List(v) => v.clone(), _ => unreachable!() };
                    let mut result = Vec::new();
                    let mut chunk = Vec::new();
                    for item in vec.iter() {
                        chunk.push(item.clone());
                        if chunk.len() == size {
                            result.push(Value::List(gc.alloc(GcData::List(chunk))));
                            chunk = Vec::new();
                        }
                    }
                    if !chunk.is_empty() {
                        result.push(Value::List(gc.alloc(GcData::List(chunk))));
                    }
                    Ok(Value::List(gc.alloc(GcData::List(result))))
                }
                v => Err(format!("list.chunk() expects list, got {}", v.type_name())),
            }
        }
        ("list", "zip") => {
            if _args.len() != 2 { return Err("list.zip() expects 2 args".into()); }
            match (&_args[0], &_args[1]) {
                (Value::List(a), Value::List(b)) => {
                    let a_vec = match gc.get(*a) { GcData::List(v) => v.clone(), _ => unreachable!() };
                    let b_vec = match gc.get(*b) { GcData::List(v) => v.clone(), _ => unreachable!() };
                    let len = a_vec.len().min(b_vec.len());
                    let mut result = Vec::new();
                    for i in 0..len {
                        result.push(Value::List(gc.alloc(GcData::List(vec![a_vec[i].clone(), b_vec[i].clone()]))));
                    }
                    Ok(Value::List(gc.alloc(GcData::List(result))))
                }
                (va, vb) => Err(format!("list.zip() expects two lists, got {} and {}", va.type_name(), vb.type_name())),
            }
        }
        ("list", "enumerate") => {
            if _args.len() != 1 { return Err("list.enumerate() expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    let vec = match gc.get(*h) { GcData::List(v) => v.clone(), _ => unreachable!() };
                    let mut result = Vec::new();
                    for (i, item) in vec.iter().enumerate() {
                        result.push(Value::List(gc.alloc(GcData::List(vec![Value::Int(i as i64), item.clone()]))));
                    }
                    Ok(Value::List(gc.alloc(GcData::List(result))))
                }
                v => Err(format!("list.enumerate() expects list, got {}", v.type_name())),
            }
        }
        ("list", "first") => {
            if _args.len() != 1 { return Err("list.first() expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get(*h) {
                        GcData::List(vec) => vec.first().cloned().ok_or_else(|| "first() on empty list".into()),
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.first() expects list, got {}", v.type_name())),
            }
        }
        ("list", "last") => {
            if _args.len() != 1 { return Err("list.last() expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get(*h) {
                        GcData::List(vec) => vec.last().cloned().ok_or_else(|| "last() on empty list".into()),
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.last() expects list, got {}", v.type_name())),
            }
        }
        ("list", "contains") => {
            if _args.len() != 2 { return Err("list.contains() expects 2 args".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get(*h) {
                        GcData::List(vec) => Ok(Value::Boolean(vec.contains(&_args[1]))),
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.contains() expects list, got {}", v.type_name())),
            }
        }
        ("list", "is_empty") => {
            if _args.len() != 1 { return Err("list.is_empty() expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get(*h) {
                        GcData::List(vec) => Ok(Value::Boolean(vec.is_empty())),
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.is_empty() expects list, got {}", v.type_name())),
            }
        }
        ("list", "insert") => {
            if _args.len() != 3 { return Err("list.insert() expects 3 args".into()); }
            match &_args[0] {
                Value::List(h) => {
                    let index = get_number(&_args[1])? as usize;
                    match gc.get_mut(*h) {
                        GcData::List(vec) => {
                            let index = index.min(vec.len());
                            vec.insert(index, _args[2].clone());
                            Ok(Value::List(*h))
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.insert() expects list, got {}", v.type_name())),
            }
        }
        ("list", "remove") => {
            if _args.len() != 2 { return Err("list.remove() expects 2 args".into()); }
            match &_args[0] {
                Value::List(h) => {
                    let index = get_number(&_args[1])? as usize;
                    match gc.get_mut(*h) {
                        GcData::List(vec) => {
                            if index >= vec.len() { return Err("list.remove() index out of bounds".into()); }
                            Ok(vec.remove(index))
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.remove() expects list, got {}", v.type_name())),
            }
        }
        ("list", "index") => {
            if _args.len() != 2 { return Err("list.index() expects 2 args".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get(*h) {
                        GcData::List(vec) => {
                            match vec.iter().position(|x| x == &_args[1]) {
                                Some(i) => Ok(Value::Int(i as i64)),
                                None => Err("Value not found in list".into()),
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.index() expects list, got {}", v.type_name())),
            }
        }
        ("list", "count") => {
            if _args.len() != 2 { return Err("list.count() expects 2 args".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get(*h) {
                        GcData::List(vec) => {
                            let count = vec.iter().filter(|x| *x == &_args[1]).count();
                            Ok(Value::Int(count as i64))
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("list.count() expects list, got {}", v.type_name())),
            }
        }
        ("list", "extend") => {
            if _args.len() != 2 { return Err("list.extend() expects 2 args".into()); }
            match (&_args[0], &_args[1]) {
                (Value::List(a), Value::List(b)) => {
                    let b_data = match gc.get(*b) {
                        GcData::List(vec) => vec.clone(),
                        _ => unreachable!(),
                    };
                    match gc.get_mut(*a) {
                        GcData::List(vec) => {
                            vec.extend(b_data);
                            Ok(Value::List(*a))
                        }
                        _ => unreachable!(),
                    }
                }
                (va, vb) => Err(format!("list.extend() expects two lists, got {} and {}", va.type_name(), vb.type_name())),
            }
        }

        // ==================== NET ====================
        ("net", "http_get") => {
            if _args.len() != 1 { return Err("net.http_get(url) expects 1 arg".into()); }
            let url = get_string(&_args[0])?;
            let resp = ureq::get(&*url).call()
                .map_err(|e| format!("HTTP GET failed: {}", e))?;
            let body = resp.into_body().read_to_string()
                .map_err(|e| format!("Failed to read response: {}", e))?;
            Ok(Value::String(body))
        }
        ("net", "http_post") => {
            if _args.len() != 2 { return Err("net.http_post(url, body) expects 2 args".into()); }
            let url = get_string(&_args[0])?;
            let body = get_string(&_args[1])?;
            let resp = ureq::post(&*url).send(body.as_bytes())
                .map_err(|e| format!("HTTP POST failed: {}", e))?;
            let resp_body = resp.into_body().read_to_string()
                .map_err(|e| format!("Failed to read response: {}", e))?;
            Ok(Value::String(resp_body))
        }
        ("net", "http_put") => {
            if _args.len() != 2 { return Err("net.http_put(url, body) expects 2 args".into()); }
            let url = get_string(&_args[0])?;
            let body = get_string(&_args[1])?;
            let resp = ureq::put(&*url).send(body.as_bytes())
                .map_err(|e| format!("HTTP PUT failed: {}", e))?;
            let resp_body = resp.into_body().read_to_string()
                .map_err(|e| format!("Failed to read response: {}", e))?;
            Ok(Value::String(resp_body))
        }
        ("net", "http_delete") => {
            if _args.len() != 1 { return Err("net.http_delete(url) expects 1 arg".into()); }
            let url = get_string(&_args[0])?;
            let resp = ureq::delete(&*url).call()
                .map_err(|e| format!("HTTP DELETE failed: {}", e))?;
            let body = resp.into_body().read_to_string()
                .map_err(|e| format!("Failed to read response: {}", e))?;
            Ok(Value::String(body))
        }
        ("net", "fetch") => {
            if _args.len() < 1 || _args.len() > 3 { return Err("net.fetch(url, ?options) expects 1-3 args".into()); }
            let url = get_string(&_args[0])?;
            let method = if _args.len() >= 2 {
                let opts = &_args[1];
                match opts {
                    Value::String(s) => s.clone(),
                    Value::Dict(h) => {
                        let pairs = match gc.get(*h) { GcData::Dict(v) => v.clone(), _ => unreachable!() };
                        let mut method = "GET".to_string();
                        let mut body_str = String::new();
                        let mut headers: Vec<(String, String)> = Vec::new();
                        for (k, v) in pairs.iter() {
                            let key = match k { Value::String(s) => s.clone(), _ => continue };
                            match key.as_str() {
                                "method" => if let Value::String(s) = v { method = s.clone(); },
                                "body" => body_str = v.to_string(),
                                "headers" => {
                                    if let Value::Dict(h2) = v {
                                        let h_pairs = match gc.get(*h2) { GcData::Dict(p) => p.clone(), _ => unreachable!() };
                                        for (hk, hv) in h_pairs.iter() {
                                            if let (Value::String(hk), Value::String(hv)) = (hk, hv) {
                                                headers.push((hk.clone(), hv.clone()));
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        // Build request
                        let agent = ureq::agent();
                        let m = method.to_uppercase();
                        let resp = match m.as_str() {
                            "GET" => {
                                let mut r = agent.get(&*url);
                                for (hk, hv) in &headers { r = r.header(hk, hv); }
                                r.call()
                            }
                            "DELETE" => {
                                let mut r = agent.delete(&*url);
                                for (hk, hv) in &headers { r = r.header(hk, hv); }
                                r.call()
                            }
                            "HEAD" => {
                                let mut r = agent.head(&*url);
                                for (hk, hv) in &headers { r = r.header(hk, hv); }
                                r.call()
                            }
                            "POST" => {
                                let mut r = agent.post(&*url);
                                for (hk, hv) in &headers { r = r.header(hk, hv); }
                                if body_str.is_empty() { r.send_empty() } else { r.send(body_str.as_bytes()) }
                            }
                            "PUT" => {
                                let mut r = agent.put(&*url);
                                for (hk, hv) in &headers { r = r.header(hk, hv); }
                                if body_str.is_empty() { r.send_empty() } else { r.send(body_str.as_bytes()) }
                            }
                            "PATCH" => {
                                let mut r = agent.patch(&*url);
                                for (hk, hv) in &headers { r = r.header(hk, hv); }
                                if body_str.is_empty() { r.send_empty() } else { r.send(body_str.as_bytes()) }
                            }
                            _ => return Err(format!("Unsupported HTTP method '{}'", method)),
                        };
                        let resp = resp.map_err(|e| format!("HTTP {} failed: {}", method, e))?;
                        let status = resp.status();
                        let mut resp_headers = Vec::new();
                        for name in &["content-type", "content-length", "location"] {
                            if let Some(val) = resp.headers().get(*name) {
                                if let Ok(s) = val.to_str() {
                                    resp_headers.push((
                                        Value::String(name.to_string()),
                                        Value::String(s.to_string()),
                                    ));
                                }
                            }
                        }
                        let resp_body = resp.into_body().read_to_string()
                            .map_err(|e| format!("Failed to read response: {}", e))?;
                        let mut result = Vec::new();
                        result.push((Value::String("status".into()), Value::Int(status.as_u16() as i64)));
                        result.push((Value::String("body".into()), Value::String(resp_body)));
                        result.push((Value::String("headers".into()), Value::Dict(gc.alloc(GcData::Dict(resp_headers)))));
                        return Ok(Value::Dict(gc.alloc(GcData::Dict(result))));
                    }
                    v => return Err(format!("net.fetch() options must be string or dict, got {}", v.type_name())),
                }
            } else {
                "GET".to_string()
            };
            let m = method.to_uppercase();
            let resp = match m.as_str() {
                "GET" => ureq::get(&*url).call(),
                "POST" => ureq::post(&*url).send(""),
                "PUT" => ureq::put(&*url).send(""),
                "DELETE" => ureq::delete(&*url).call(),
                _ => return Err(format!("Unsupported HTTP method '{}'", method)),
            };
            let resp = resp.map_err(|e| format!("HTTP {} failed: {}", method, e))?;
            let body = resp.into_body().read_to_string()
                .map_err(|e| format!("Failed to read response: {}", e))?;
            Ok(Value::String(body))
        }
        ("net", "tcp_connect") => {
            if _args.len() != 2 { return Err("net.tcp_connect(host, port) expects 2 args".into()); }
            let host = get_string(&_args[0])?;
            let port = get_number(&_args[1])? as u16;
            let addr = format!("{}:{}", host, port);
            let mut stream = std::net::TcpStream::connect(&addr)
                .map_err(|e| format!("TCP connect to '{}' failed: {}", addr, e))?;
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .ok();
            // Read available data (up to 64KB)
            let mut buf = vec![0u8; 65536];
            let n = stream.read(&mut buf)
                .map_err(|e| format!("TCP read failed: {}", e))?;
            buf.truncate(n);
            let text = String::from_utf8_lossy(&buf).to_string();
            Ok(Value::String(text))
        }
        ("net", "dns_lookup") => {
            if _args.len() != 1 { return Err("net.dns_lookup(host) expects 1 arg".into()); }
            let host = get_string(&_args[0])?;
            use std::net::ToSocketAddrs;
            let addrs: Vec<Value> = (host.as_ref(), 0u16)
                .to_socket_addrs()
                .map_err(|e| format!("DNS lookup failed for '{}': {}", host, e))?
                .map(|addr| Value::String(addr.to_string()))
                .collect();
            Ok(Value::List(gc.alloc(GcData::List(addrs))))
        }

        // ==================== RANDOM ====================
        ("random", "seed") => {
            if _args.len() != 1 { return Err("random.seed() expects 1 arg".into()); }
            let s = get_number(&_args[0])? as u64;
            RNG.with(|rng| *rng.borrow_mut() = SimpleRng::new(s));
            Ok(Value::Nil)
        }
        ("random", "int") => {
            if _args.len() != 2 { return Err("random.int(min, max) expects 2 args".into()); }
            let lo = get_number(&_args[0])? as i64;
            let hi = get_number(&_args[1])? as i64;
            let val = RNG.with(|rng| rng.borrow_mut().next_range(lo, hi + 1));
            Ok(Value::Int(val))
        }
        ("random", "float") => {
            if _args.len() != 2 { return Err("random.float(min, max) expects 2 args".into()); }
            let lo = get_number(&_args[0])?;
            let hi = get_number(&_args[1])?;
            let val = RNG.with(|rng| {
                let r = rng.borrow_mut().next_f64();
                lo + r * (hi - lo)
            });
            Ok(Value::Float(val))
        }
        ("random", "choice") => {
            if _args.len() != 1 { return Err("random.choice(list) expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    match gc.get(*h) {
                        GcData::List(vec) => {
                            if vec.is_empty() { return Err("random.choice() on empty list".into()); }
                            let idx = RNG.with(|rng| rng.borrow_mut().next_range(0, vec.len() as i64));
                            Ok(vec[idx as usize].clone())
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("random.choice() expects list, got {}", v.type_name())),
            }
        }
        ("random", "shuffle") => {
            if _args.len() != 1 { return Err("random.shuffle(list) expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    let len = match gc.get(*h) { GcData::List(v) => v.len(), _ => unreachable!() };
                    let indices: Vec<(usize, usize)> = RNG.with(|r| {
                        let mut rng = r.borrow_mut();
                        (1..len).rev().map(|i| {
                            let j = rng.next_range(0, (i + 1) as i64) as usize;
                            (i, j)
                        }).collect()
                    });
                    let vec = match gc.get_mut(*h) { GcData::List(v) => v, _ => unreachable!() };
                    for (i, j) in indices {
                        vec.swap(i, j);
                    }
                    Ok(Value::List(*h))
                }
                v => Err(format!("random.shuffle() expects list, got {}", v.type_name())),
            }
        }
        ("random", "uuid") => {
            Ok(Value::String(uuid_v4()))
        }
        ("random", "normal") => {
            // Box-Muller transform
            let (u1, u2) = RNG.with(|rng| {
                let mut r = rng.borrow_mut();
                (r.next_f64(), r.next_f64())
            });
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            Ok(Value::Float(z))
        }
        ("random", "bytes") => {
            if _args.len() != 1 { return Err("random.bytes(n) expects 1 arg".into()); }
            let n = get_number(&_args[0])? as usize;
            let mut rng = SimpleRng::new(
                SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
            );
            let mut bytes = vec![0u8; n];
            for b in &mut bytes {
                *b = (rng.next_u64() & 0xFF) as u8;
            }
            // Store as a list of ints
            let vals: Vec<Value> = bytes.into_iter().map(|b| Value::Int(b as i64)).collect();
            Ok(Value::List(gc.alloc(GcData::List(vals))))
        }

        // ==================== ENCODING ====================
        ("encoding", "base64_encode") => {
            if _args.len() != 1 { return Err("encoding.base64_encode() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            Ok(Value::String(base64_encode(s.as_bytes())))
        }
        ("encoding", "base64_decode") => {
            if _args.len() != 1 { return Err("encoding.base64_decode() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            match base64_decode(s) {
                Ok(bytes) => Ok(Value::String(String::from_utf8_lossy(&bytes).to_string())),
                Err(e) => Err(format!("base64 decode failed: {}", e)),
            }
        }
        ("encoding", "hex_encode") => {
            if _args.len() != 1 { return Err("encoding.hex_encode() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            Ok(Value::String(hex_encode(s.as_bytes())))
        }
        ("encoding", "hex_decode") => {
            if _args.len() != 1 { return Err("encoding.hex_decode() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            match hex_decode(s) {
                Ok(bytes) => Ok(Value::String(String::from_utf8_lossy(&bytes).to_string())),
                Err(e) => Err(format!("hex decode failed: {}", e)),
            }
        }

        // ==================== SET ====================
        ("set", "add") => {
            if _args.len() != 2 { return Err("set.add() expects 2 args".into()); }
            match &_args[0] {
                Value::Set(h) => {
                    match gc.get_mut(*h) {
                        GcData::Set(vec) => {
                            if !vec.contains(&_args[1]) {
                                vec.push(_args[1].clone());
                            }
                            Ok(Value::Set(*h))
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("set.add() expects set, got {}", v.type_name())),
            }
        }
        ("set", "remove") => {
            if _args.len() != 2 { return Err("set.remove() expects 2 args".into()); }
            match &_args[0] {
                Value::Set(h) => {
                    match gc.get_mut(*h) {
                        GcData::Set(vec) => {
                            let idx = vec.iter().position(|x| x == &_args[1]);
                            match idx {
                                Some(i) => { vec.remove(i); Ok(Value::Set(*h)) }
                                None => Err("Element not found in set".into()),
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("set.remove() expects set, got {}", v.type_name())),
            }
        }
        ("set", "contains") => {
            if _args.len() != 2 { return Err("set.contains() expects 2 args".into()); }
            match &_args[0] {
                Value::Set(h) => {
                    match gc.get(*h) {
                        GcData::Set(vec) => Ok(Value::Boolean(vec.contains(&_args[1]))),
                        _ => unreachable!(),
                    }
                }
                v => Err(format!("set.contains() expects set, got {}", v.type_name())),
            }
        }
        ("set", "union") => {
            if _args.len() != 2 { return Err("set.union() expects 2 args".into()); }
            match (&_args[0], &_args[1]) {
                (Value::Set(a), Value::Set(b)) => {
                    let a_vec = match gc.get(*a) { GcData::Set(v) => v.clone(), _ => unreachable!() };
                    let b_vec = match gc.get(*b) { GcData::Set(v) => v.clone(), _ => unreachable!() };
                    let mut result = a_vec;
                    for item in b_vec.iter() {
                        if !result.contains(item) {
                            result.push(item.clone());
                        }
                    }
                    Ok(Value::Set(gc.alloc(GcData::Set(result))))
                }
                (va, vb) => Err(format!("set.union() expects two sets, got {} and {}", va.type_name(), vb.type_name())),
            }
        }
        ("set", "intersection") => {
            if _args.len() != 2 { return Err("set.intersection() expects 2 args".into()); }
            match (&_args[0], &_args[1]) {
                (Value::Set(a), Value::Set(b)) => {
                    let a_vec = match gc.get(*a) { GcData::Set(v) => v.clone(), _ => unreachable!() };
                    let b_vec = match gc.get(*b) { GcData::Set(v) => v.clone(), _ => unreachable!() };
                    let result: Vec<Value> = a_vec.iter().filter(|x| b_vec.contains(x)).cloned().collect();
                    Ok(Value::Set(gc.alloc(GcData::Set(result))))
                }
                (va, vb) => Err(format!("set.intersection() expects two sets, got {} and {}", va.type_name(), vb.type_name())),
            }
        }
        ("set", "difference") => {
            if _args.len() != 2 { return Err("set.difference() expects 2 args".into()); }
            match (&_args[0], &_args[1]) {
                (Value::Set(a), Value::Set(b)) => {
                    let a_vec = match gc.get(*a) { GcData::Set(v) => v.clone(), _ => unreachable!() };
                    let b_vec = match gc.get(*b) { GcData::Set(v) => v.clone(), _ => unreachable!() };
                    let result: Vec<Value> = a_vec.iter().filter(|x| !b_vec.contains(x)).cloned().collect();
                    Ok(Value::Set(gc.alloc(GcData::Set(result))))
                }
                (va, vb) => Err(format!("set.difference() expects two sets, got {} and {}", va.type_name(), vb.type_name())),
            }
        }
        ("set", "is_subset") => {
            if _args.len() != 2 { return Err("set.is_subset() expects 2 args".into()); }
            match (&_args[0], &_args[1]) {
                (Value::Set(a), Value::Set(b)) => {
                    let a_vec = match gc.get(*a) { GcData::Set(v) => v, _ => unreachable!() };
                    let b_vec = match gc.get(*b) { GcData::Set(v) => v, _ => unreachable!() };
                    Ok(Value::Boolean(a_vec.iter().all(|x| b_vec.contains(x))))
                }
                (va, vb) => Err(format!("set.is_subset() expects two sets, got {} and {}", va.type_name(), vb.type_name())),
            }
        }
        ("set", "to_list") => {
            if _args.len() != 1 { return Err("set.to_list() expects 1 arg".into()); }
            match &_args[0] {
                Value::Set(h) => {
                    let items = match gc.get(*h) { GcData::Set(v) => v.clone(), _ => unreachable!() };
                    Ok(Value::List(gc.alloc(GcData::List(items))))
                }
                v => Err(format!("set.to_list() expects set, got {}", v.type_name())),
            }
        }

        // ==================== REGEX ====================
        ("regex", "is_match") => {
            if _args.len() != 2 { return Err("regex.is_match() expects 2 args".into()); }
            let pattern = get_string(&_args[0])?;
            let text = get_string(&_args[1])?;
            let re = Regex::new(pattern).map_err(|e| format!("Invalid regex: {}", e))?;
            Ok(Value::Boolean(re.is_match(text)))
        }
        ("regex", "find") => {
            if _args.len() != 2 { return Err("regex.find() expects 2 args".into()); }
            let pattern = get_string(&_args[0])?;
            let text = get_string(&_args[1])?;
            let re = Regex::new(pattern).map_err(|e| format!("Invalid regex: {}", e))?;
            match re.find(text) {
                Some(m) => Ok(Value::String(m.as_str().to_string())),
                None => Ok(Value::Nil),
            }
        }
        ("regex", "find_all") => {
            if _args.len() != 2 { return Err("regex.find_all() expects 2 args".into()); }
            let pattern = get_string(&_args[0])?;
            let text = get_string(&_args[1])?;
            let re = Regex::new(pattern).map_err(|e| format!("Invalid regex: {}", e))?;
            let results: Vec<Value> = re.find_iter(text).map(|m| Value::String(m.as_str().to_string())).collect();
            Ok(Value::List(gc.alloc(GcData::List(results))))
        }
        ("regex", "replace") => {
            if _args.len() != 3 { return Err("regex.replace() expects 3 args".into()); }
            let pattern = get_string(&_args[0])?;
            let text = get_string(&_args[1])?;
            let replacement = get_string(&_args[2])?;
            let re = Regex::new(pattern).map_err(|e| format!("Invalid regex: {}", e))?;
            Ok(Value::String(re.replace_all(text, replacement).to_string()))
        }
        ("regex", "split") => {
            if _args.len() != 2 { return Err("regex.split() expects 2 args".into()); }
            let pattern = get_string(&_args[0])?;
            let text = get_string(&_args[1])?;
            let re = Regex::new(pattern).map_err(|e| format!("Invalid regex: {}", e))?;
            let parts: Vec<Value> = re.split(text).map(|s| Value::String(s.to_string())).collect();
            Ok(Value::List(gc.alloc(GcData::List(parts))))
        }

        // ==================== PROCESS ====================
        ("process", "run") => {
            if _args.is_empty() { return Err("process.run() expects at least 1 arg".into()); }
            let cmd = get_string(&_args[0])?.to_string();
            let mut command = std::process::Command::new(&cmd);
            for arg in &_args[1..] {
                command.arg(get_string(arg)?);
            }
            let status = command.status().map_err(|e| format!("Could not run '{}': {}", cmd, e))?;
            Ok(Value::Int(status.code().unwrap_or(-1) as i64))
        }
        ("process", "output") => {
            if _args.is_empty() { return Err("process.output() expects at least 1 arg".into()); }
            let cmd = get_string(&_args[0])?.to_string();
            let mut command = std::process::Command::new(&cmd);
            for arg in &_args[1..] {
                command.arg(get_string(arg)?);
            }
            let output = command.output().map_err(|e| format!("Could not run '{}': {}", cmd, e))?;
            let mut dict = Vec::new();
            dict.push((Value::String("stdout".into()), Value::String(String::from_utf8_lossy(&output.stdout).to_string())));
            dict.push((Value::String("stderr".into()), Value::String(String::from_utf8_lossy(&output.stderr).to_string())));
            dict.push((Value::String("status".into()), Value::Int(output.status.code().unwrap_or(-1) as i64)));
            Ok(Value::Dict(gc.alloc(GcData::Dict(dict))))
        }
        ("process", "spawn") => {
            if _args.is_empty() { return Err("process.spawn() expects at least 1 arg".into()); }
            let cmd = get_string(&_args[0])?.to_string();
            let mut command = std::process::Command::new(&cmd);
            for arg in &_args[1..] {
                command.arg(get_string(arg)?);
            }
            let child = command.spawn().map_err(|e| format!("Could not spawn '{}': {}", cmd, e))?;
            let pid = child.id();
            // Detach — let OS clean up
            std::mem::drop(child);
            Ok(Value::Int(pid as i64))
        }

        // ==================== HASHLIB ====================
        ("hashlib", "md5") => {
            if _args.len() != 1 { return Err("hashlib.md5() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            Ok(Value::String(md5_hash(s.as_bytes())))
        }
        ("hashlib", "sha1") => {
            if _args.len() != 1 { return Err("hashlib.sha1() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            Ok(Value::String(sha1_hash(s.as_bytes())))
        }
        ("hashlib", "sha256") => {
            if _args.len() != 1 { return Err("hashlib.sha256() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            Ok(Value::String(sha256_hash(s.as_bytes())))
        }
        ("hashlib", "sha512") => {
            if _args.len() != 1 { return Err("hashlib.sha512() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            Ok(Value::String(sha512_hash(s.as_bytes())))
        }

        // ==================== PATH ====================
        ("path", "join") => {
            if _args.is_empty() { return Err("path.join() expects at least 1 arg".into()); }
            let parts: Result<Vec<&str>, String> = _args.iter().map(|a| get_string(a)).collect();
            let parts = parts?;
            let mut p = std::path::PathBuf::new();
            for part in parts {
                p.push(part);
            }
            Ok(Value::String(p.to_string_lossy().to_string()))
        }
        ("path", "dirname") => {
            if _args.len() != 1 { return Err("path.dirname() expects 1 arg".into()); }
            let p = Path::new(get_string(&_args[0])?);
            Ok(Value::String(p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_else(|| "".into())))
        }
        ("path", "basename") => {
            if _args.len() != 1 { return Err("path.basename() expects 1 arg".into()); }
            let p = Path::new(get_string(&_args[0])?);
            Ok(Value::String(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "".into())))
        }
        ("path", "extension") => {
            if _args.len() != 1 { return Err("path.extension() expects 1 arg".into()); }
            let p = Path::new(get_string(&_args[0])?);
            match p.extension() {
                Some(e) => Ok(Value::String(e.to_string_lossy().to_string())),
                None => Ok(Value::Nil),
            }
        }
        ("path", "stem") => {
            if _args.len() != 1 { return Err("path.stem() expects 1 arg".into()); }
            let p = Path::new(get_string(&_args[0])?);
            match p.file_stem() {
                Some(s) => Ok(Value::String(s.to_string_lossy().to_string())),
                None => Ok(Value::Nil),
            }
        }
        ("path", "parent") => {
            if _args.len() != 1 { return Err("path.parent() expects 1 arg".into()); }
            let p = Path::new(get_string(&_args[0])?);
            match p.parent() {
                Some(d) => Ok(Value::String(d.to_string_lossy().to_string())),
                None => Ok(Value::Nil),
            }
        }
        ("path", "split") => {
            if _args.len() != 1 { return Err("path.split() expects 1 arg".into()); }
            let p = Path::new(get_string(&_args[0])?);
            let parent = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_else(|| "".into());
            let file = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "".into());
            Ok(Value::List(gc.alloc(GcData::List(vec![Value::String(parent), Value::String(file)]))))
        }
        ("path", "resolve") => {
            if _args.len() != 1 { return Err("path.resolve() expects 1 arg".into()); }
            let p = Path::new(get_string(&_args[0])?);
            match std::path::absolute(p) {
                Ok(abs) => Ok(Value::String(abs.to_string_lossy().to_string())),
                Err(e) => Err(format!("path.resolve() failed: {}", e)),
            }
        }
        ("path", "relative") => {
            if _args.len() != 2 { return Err("path.relative() expects 2 args".into()); }
            let from = Path::new(get_string(&_args[0])?);
            let to = Path::new(get_string(&_args[1])?);
            match from.strip_prefix(to).or_else(|_| to.strip_prefix(from)) {
                Ok(rel) => Ok(Value::String(rel.to_string_lossy().to_string())),
                Err(_) => Ok(Value::String(get_string(&_args[1])?.to_string())),
            }
        }
        ("path", "normalize") => {
            if _args.len() != 1 { return Err("path.normalize() expects 1 arg".into()); }
            let p = Path::new(get_string(&_args[0])?);
            let mut components: Vec<&str> = Vec::new();
            for c in p.components() {
                match c {
                    std::path::Component::Normal(s) => components.push(std::ffi::OsStr::to_str(s).unwrap_or("")),
                    std::path::Component::ParentDir => { components.pop(); }
                    _ => {}
                }
            }
            let result = if p.is_absolute() {
                "/".to_string() + &components.join("/")
            } else {
                components.join("/")
            };
            Ok(Value::String(result))
        }
        ("path", "is_absolute") => {
            if _args.len() != 1 { return Err("path.is_absolute() expects 1 arg".into()); }
            Ok(Value::Boolean(Path::new(get_string(&_args[0])?).is_absolute()))
        }

        // ==================== CSV ====================
        ("csv", "parse") => {
            if _args.len() != 1 { return Err("csv.parse() expects 1 arg".into()); }
            let s = get_string(&_args[0])?;
            let mut rows = Vec::new();
            for line in s.lines() {
                if line.trim().is_empty() { continue; }
                let fields: Vec<Value> = parse_csv_line(line).into_iter().map(Value::String).collect();
                rows.push(Value::List(gc.alloc(GcData::List(fields))));
            }
            Ok(Value::List(gc.alloc(GcData::List(rows))))
        }
        ("csv", "encode") => {
            if _args.len() != 1 { return Err("csv.encode() expects 1 arg".into()); }
            match &_args[0] {
                Value::List(h) => {
                    let rows = match gc.get(*h) { GcData::List(v) => v.clone(), _ => unreachable!() };
                    let mut out = String::new();
                    for row in &rows {
                        match row {
                            Value::List(ch) => {
                                let fields = match gc.get(*ch) { GcData::List(v) => v.clone(), _ => unreachable!() };
                                out.push_str(&fields.iter().map(|f| encode_csv_field(&f.to_string())).collect::<Vec<_>>().join(","));
                                out.push('\n');
                            }
                            v => out.push_str(&encode_csv_field(&v.to_string())),
                        }
                    }
                    Ok(Value::String(out))
                }
                v => Err(format!("csv.encode() expects list, got {}", v.type_name())),
            }
        }
        ("csv", "parse_file") => {
            if _args.len() != 1 { return Err("csv.parse_file() expects 1 arg".into()); }
            let path = get_string(&_args[0])?;
            let content = fs::read_to_string(Path::new(path))
                .map_err(|e| format!("Could not read '{}': {}", path, e))?;
            let mut rows = Vec::new();
            for line in content.lines() {
                if line.trim().is_empty() { continue; }
                let fields: Vec<Value> = parse_csv_line(line).into_iter().map(Value::String).collect();
                rows.push(Value::List(gc.alloc(GcData::List(fields))));
            }
            Ok(Value::List(gc.alloc(GcData::List(rows))))
        }
        ("csv", "encode_file") => {
            if _args.len() != 2 { return Err("csv.encode_file() expects 2 args".into()); }
            let path = get_string(&_args[0])?.to_string();
            match &_args[1] {
                Value::List(h) => {
                    let rows = match gc.get(*h) { GcData::List(v) => v.clone(), _ => unreachable!() };
                    let mut out = String::new();
                    for row in &rows {
                        match row {
                            Value::List(ch) => {
                                let fields = match gc.get(*ch) { GcData::List(v) => v.clone(), _ => unreachable!() };
                                out.push_str(&fields.iter().map(|f| encode_csv_field(&f.to_string())).collect::<Vec<_>>().join(","));
                                out.push('\n');
                            }
                            v => out.push_str(&encode_csv_field(&v.to_string())),
                        }
                    }
                    fs::write(Path::new(&path), &out)
                        .map_err(|e| format!("Could not write '{}': {}", path, e))?;
                    Ok(Value::Nil)
                }
                v => Err(format!("csv.encode_file() expects list as second arg, got {}", v.type_name())),
            }
        }

        // ==================== LOGGING ====================
        ("logging", "set_level") => {
            if _args.len() != 1 { return Err("logging.set_level() expects 1 arg".into()); }
            let level = get_string(&_args[0])?;
            let lvl = match level {
                "debug" => 0,
                "info" => 1,
                "warn" => 2,
                "error" => 3,
                "fatal" => 4,
                _ => return Err(format!("Unknown log level '{}'", level)),
            };
            LOG_LEVEL.store(lvl, Ordering::Relaxed);
            Ok(Value::Nil)
        }
        ("logging", "debug") => {
            if _args.len() != 1 { return Err("logging.debug() expects 1 arg".into()); }
            let level_idx = 0;
            let current = LOG_LEVEL.load(Ordering::Relaxed);
            if level_idx < current { return Ok(Value::Nil); }
            let msg = get_string(&_args[0])?;
            let log_msg = format!("[DEBUG] {}", msg);
            let log_path = log_path().lock().unwrap();
            if let Some(path) = log_path.as_ref() {
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(Path::new(path))
                    .map_err(|e| format!("Log file error: {}", e))?;
                writeln!(file, "{}", log_msg).map_err(|e| format!("Log write error: {}", e))?;
            } else {
                eprintln!("{}", log_msg);
            }
            Ok(Value::Nil)
        }
        ("logging", "info") => {
            if _args.len() != 1 { return Err("logging.info() expects 1 arg".into()); }
            let level_idx = 1;
            let current = LOG_LEVEL.load(Ordering::Relaxed);
            if level_idx < current { return Ok(Value::Nil); }
            let msg = get_string(&_args[0])?;
            let log_msg = format!("[INFO] {}", msg);
            let log_path = log_path().lock().unwrap();
            if let Some(path) = log_path.as_ref() {
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(Path::new(path))
                    .map_err(|e| format!("Log file error: {}", e))?;
                writeln!(file, "{}", log_msg).map_err(|e| format!("Log write error: {}", e))?;
            } else {
                eprintln!("{}", log_msg);
            }
            Ok(Value::Nil)
        }
        ("logging", "warn") => {
            if _args.len() != 1 { return Err("logging.warn() expects 1 arg".into()); }
            let level_idx = 2;
            let current = LOG_LEVEL.load(Ordering::Relaxed);
            if level_idx < current { return Ok(Value::Nil); }
            let msg = get_string(&_args[0])?;
            let log_msg = format!("[WARN] {}", msg);
            let log_path = log_path().lock().unwrap();
            if let Some(path) = log_path.as_ref() {
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(Path::new(path))
                    .map_err(|e| format!("Log file error: {}", e))?;
                writeln!(file, "{}", log_msg).map_err(|e| format!("Log write error: {}", e))?;
            } else {
                eprintln!("{}", log_msg);
            }
            Ok(Value::Nil)
        }
        ("logging", "error") => {
            if _args.len() != 1 { return Err("logging.error() expects 1 arg".into()); }
            let level_idx = 3;
            let current = LOG_LEVEL.load(Ordering::Relaxed);
            if level_idx < current { return Ok(Value::Nil); }
            let msg = get_string(&_args[0])?;
            let log_msg = format!("[ERROR] {}", msg);
            let log_path = log_path().lock().unwrap();
            if let Some(path) = log_path.as_ref() {
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(Path::new(path))
                    .map_err(|e| format!("Log file error: {}", e))?;
                writeln!(file, "{}", log_msg).map_err(|e| format!("Log write error: {}", e))?;
            } else {
                eprintln!("{}", log_msg);
            }
            Ok(Value::Nil)
        }
        ("logging", "fatal") => {
            if _args.len() != 1 { return Err("logging.fatal() expects 1 arg".into()); }
            let level_idx = 4;
            let current = LOG_LEVEL.load(Ordering::Relaxed);
            if level_idx < current { return Ok(Value::Nil); }
            let msg = get_string(&_args[0])?;
            let log_msg = format!("[FATAL] {}", msg);
            let log_path = log_path().lock().unwrap();
            if let Some(path) = log_path.as_ref() {
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(Path::new(path))
                    .map_err(|e| format!("Log file error: {}", e))?;
                writeln!(file, "{}", log_msg).map_err(|e| format!("Log write error: {}", e))?;
            } else {
                eprintln!("{}", log_msg);
            }
            std::process::exit(1);
        }
        ("logging", "set_file") => {
            if _args.len() != 1 { return Err("logging.set_file() expects 1 arg".into()); }
            let path = get_string(&_args[0])?.to_string();
            *log_path().lock().unwrap() = Some(path);
            Ok(Value::Nil)
        }
        ("logging", "set_format") => {
            if _args.len() != 1 { return Err("logging.set_format() expects 1 arg".into()); }
            let fmt = get_string(&_args[0])?.to_string();
            *log_format().lock().unwrap() = fmt;
            Ok(Value::Nil)
        }

        // ==================== THREADING ====================
        ("threading", "mutex") => {
            if !_args.is_empty() { return Err("threading.mutex() expects 0 args".into()); }
            let id = THREADING_ID.fetch_add(1, Ordering::Relaxed);
            let mutex = Arc::new(std::sync::Mutex::new(Value::Nil));
            threading_mutexes().lock().unwrap().insert(id, mutex);
            Ok(Value::Int(id as i64))
        }
        ("threading", "lock") => {
            if _args.len() != 1 { return Err("threading.lock() expects 1 arg".into()); }
            let id = get_number(&_args[0])? as u64;
            let m = threading_mutexes().lock().unwrap().get(&id).ok_or_else(|| "Mutex not found".to_string())?.clone();
            match m.lock() {
                Ok(_) => Ok(Value::Nil),
                Err(e) => Err(format!("Mutex lock failed: {}", e)),
            }
        }
        ("threading", "try_lock") => {
            if _args.len() != 1 { return Err("threading.try_lock() expects 1 arg".into()); }
            let id = get_number(&_args[0])? as u64;
            let m = threading_mutexes().lock().unwrap().get(&id).ok_or_else(|| "Mutex not found".to_string())?.clone();
            Ok(Value::Boolean(m.try_lock().is_ok()))
        }
        ("threading", "unlock") => {
            if _args.len() != 1 { return Err("threading.unlock() expects 1 arg".into()); }
            let id = get_number(&_args[0])? as u64;
            let _m = threading_mutexes().lock().unwrap().get(&id).ok_or_else(|| "Mutex not found".to_string())?.clone();
            Ok(Value::Nil)
        }
        ("threading", "semaphore") => {
            if _args.len() != 1 { return Err("threading.semaphore() expects 1 arg".into()); }
            let max = get_number(&_args[0])? as usize;
            let id = THREADING_ID.fetch_add(1, Ordering::Relaxed);
            let sem = Arc::new(SemaphoreInner::new(max));
            threading_semaphores().lock().unwrap().insert(id, sem);
            Ok(Value::Int(id as i64))
        }
        ("threading", "acquire") => {
            if _args.len() != 1 { return Err("threading.acquire() expects 1 arg".into()); }
            let id = get_number(&_args[0])? as u64;
            let map = threading_semaphores().lock().unwrap();
            let sem = map.get(&id).ok_or_else(|| "Semaphore not found".to_string())?.clone();
            drop(map);
            sem.acquire();
            Ok(Value::Nil)
        }
        ("threading", "try_acquire") => {
            if _args.len() != 1 { return Err("threading.try_acquire() expects 1 arg".into()); }
            let id = get_number(&_args[0])? as u64;
            let map = threading_semaphores().lock().unwrap();
            let sem = map.get(&id).ok_or_else(|| "Semaphore not found".to_string())?.clone();
            drop(map);
            Ok(Value::Boolean(sem.try_acquire()))
        }
        ("threading", "release") => {
            if _args.len() != 1 { return Err("threading.release() expects 1 arg".into()); }
            let id = get_number(&_args[0])? as u64;
            let map = threading_semaphores().lock().unwrap();
            let sem = map.get(&id).ok_or_else(|| "Semaphore not found".to_string())?.clone();
            drop(map);
            sem.release();
            Ok(Value::Nil)
        }
        ("threading", "rwlock") => {
            if !_args.is_empty() { return Err("threading.rwlock() expects 0 args".into()); }
            let id = THREADING_ID.fetch_add(1, Ordering::Relaxed);
            let lock = Arc::new(std::sync::RwLock::new(Value::Nil));
            threading_rwlocks().lock().unwrap().insert(id, lock);
            Ok(Value::Int(id as i64))
        }
        ("threading", "read_lock") => {
            if _args.len() != 1 { return Err("threading.read_lock() expects 1 arg".into()); }
            let id = get_number(&_args[0])? as u64;
            let map = threading_rwlocks().lock().unwrap();
            let lock = map.get(&id).ok_or_else(|| "RWLock not found".to_string())?.clone();
            drop(map);
            drop(lock.read().map_err(|e| format!("RWLock read lock failed: {}", e))?);
            Ok(Value::Nil)
        }
        ("threading", "read_unlock") => {
            if _args.len() != 1 { return Err("threading.read_unlock() expects 1 arg".into()); }
            Ok(Value::Nil) // no-op, lock guard dropped
        }
        ("threading", "write_lock") => {
            if _args.len() != 1 { return Err("threading.write_lock() expects 1 arg".into()); }
            let id = get_number(&_args[0])? as u64;
            let map = threading_rwlocks().lock().unwrap();
            let lock = map.get(&id).ok_or_else(|| "RWLock not found".to_string())?.clone();
            drop(map);
            drop(lock.write().map_err(|e| format!("RWLock write lock failed: {}", e))?);
            Ok(Value::Nil)
        }
        ("threading", "write_unlock") => {
            if _args.len() != 1 { return Err("threading.write_unlock() expects 1 arg".into()); }
            Ok(Value::Nil) // no-op, lock guard dropped
        }
        ("threading", "synchronized") => {
            if _args.len() != 2 { return Err("threading.synchronized() expects 2 args (mutex_id, fn)".into()); }
            let id = get_number(&_args[0])? as u64;
            let map = threading_mutexes().lock().unwrap();
            let m = map.get(&id).ok_or_else(|| "Mutex not found".to_string())?.clone();
            drop(map);
            let _guard = m.lock().map_err(|e| format!("Mutex lock failed: {}", e))?;
            // Our lock can't hold a guard across eval boundary, so this is best-effort
            Ok(Value::Nil)
        }

        _ => Err(format!("Unknown stdlib function '{}.{}'", module, name)),
    }
}

fn one_num(args: &[Value], f: fn(f64) -> f64) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Expected 1 argument".into());
    }
    args[0].as_float().map(|n| Value::Float(f(n)))
        .ok_or_else(|| "Expected number".into())
}

fn two_num(args: &[Value], f: fn(f64, f64) -> f64) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("Expected 2 arguments".into());
    }
    match (args[0].as_float(), args[1].as_float()) {
        (Some(a), Some(b)) => Ok(Value::Float(f(a, b))),
        _ => Err("Expected numbers".into()),
    }
}

thread_local! {
    static RNG: std::cell::RefCell<SimpleRng> = std::cell::RefCell::new(SimpleRng::new(42));
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        SimpleRng { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    fn next_range(&mut self, lo: i64, hi: i64) -> i64 {
        if lo >= hi { return lo; }
        let range = (hi - lo) as u64;
        lo + (self.next_u64() % range) as i64
    }
}

fn uuid_v4() -> String {
    let mut rng = SimpleRng::new(
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
    );
    let mut bytes = [0u8; 16];
    for b in &mut bytes {
        *b = (rng.next_u64() & 0xFF) as u8;
    }
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11],
        bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim_end_matches('=');
    let mut result = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for &c in s.as_bytes() {
        let val = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(format!("invalid base64 character '{}'", c as char)),
        } as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(result)
}

fn hex_encode(data: &[u8]) -> String {
    let mut result = String::with_capacity(data.len() * 2);
    for b in data {
        result.push_str(&format!("{:02x}", b));
    }
    result
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 { return Err("hex string length must be even".into()); }
    let s = s.trim();
    let mut result = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks(2) {
        let hi = hex_val(chunk[0])?;
        let lo = hex_val(chunk[1])?;
        result.push((hi << 4) | lo);
    }
    Ok(result)
}

fn hex_val(c: u8) -> Result<u8, String> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(format!("invalid hex character '{}'", c as char)),
    }
}

fn unix_timestamp() -> Result<i64, String> {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    Ok(dur.as_secs() as i64)
}

fn unix_ts_to_components(ts: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = if ts >= 0 { ts / 86400 } else { (ts - 86399) / 86400 };
    let time = ts.rem_euclid(86400);
    let h = (time / 3600) as u32;
    let mi = ((time % 3600) / 60) as u32;
    let s = (time % 60) as u32;

    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo as u32, d as u32, h, mi, s)
}

// ==================== HASHLIB HELPERS ====================

fn md5_hash(data: &[u8]) -> String {
    let mut state: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    let shift = [7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
                 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
                 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
                 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21];
    let tables: [[u32; 16]; 4] = [
        [0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
         0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821],
        [0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
         0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a],
        [0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
         0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665],
        [0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
         0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391],
    ];
    let mut msg = data.to_vec();
    let orig_len = msg.len() as u64;
    msg.push(0x80);
    while (msg.len() * 8) % 512 != 448 {
        msg.push(0);
    }
    msg.extend_from_slice(&orig_len.to_le_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 16];
        for (i, word) in w.iter_mut().enumerate() {
            *word = u32::from_le_bytes(chunk[i * 4..][..4].try_into().unwrap());
        }
        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(tables[i / 16][i % 16]).wrapping_add(w[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(shift[i]));
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }
    state.iter().flat_map(|n| n.to_le_bytes()).map(|b| format!("{:02x}", b)).collect()
}

fn sha1_hash(data: &[u8]) -> String {
    let mut h: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
    let mut msg = data.to_vec();
    let orig_len = msg.len() as u64;
    msg.push(0x80);
    while ((msg.len() * 8) % 512) != 448 {
        msg.push(0);
    }
    msg.extend_from_slice(&orig_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..][..4].try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for i in 0..80 {
            let (f, k) = match i / 20 {
                0 => ((b & c) | (!b & d), 0x5a827999),
                1 => (b ^ c ^ d, 0x6ed9eba1),
                2 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    h.iter().map(|n| format!("{:08x}", n)).collect()
}

fn sha256_hash(data: &[u8]) -> String {
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
        0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
        0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
        0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let orig_len = msg.len() as u64;
    msg.push(0x80);
    while ((msg.len() * 8) % 512) != 448 {
        msg.push(0);
    }
    msg.extend_from_slice(&orig_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..][..4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) = (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|n| format!("{:08x}", n)).collect()
}

fn sha512_hash(data: &[u8]) -> String {
    let k: [u64; 80] = [
        0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
        0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
        0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
        0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
        0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
        0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
        0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
        0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
        0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
        0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
        0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
        0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
        0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
        0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
        0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
        0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
        0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
        0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
        0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
        0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
    ];
    let mut h: [u64; 8] = [
        0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1, 0x510e527fade682d1, 0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
    ];
    let mut msg = data.to_vec();
    let orig_len = msg.len() as u128;
    msg.push(0x80);
    while ((msg.len() * 8) % 1024) != 896 {
        msg.push(0);
    }
    msg.extend_from_slice(&(orig_len as u64).to_be_bytes());
    msg.extend_from_slice(&(0u64).to_be_bytes());
    for chunk in msg.chunks(128) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            w[i] = u64::from_be_bytes(chunk[i * 8..][..8].try_into().unwrap());
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) = (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|n| format!("{:016x}", n)).collect()
}

// ==================== CSV HELPERS ====================

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if !in_quotes => in_quotes = true,
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

fn encode_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

// ==================== LOGGING GLOBALS ====================

static LOG_LEVEL: AtomicU64 = AtomicU64::new(1); // default: info
static LOG_PATH: OnceLock<std::sync::Mutex<Option<String>>> = OnceLock::new();
static LOG_FORMAT: OnceLock<std::sync::Mutex<String>> = OnceLock::new();

fn log_path() -> &'static std::sync::Mutex<Option<String>> {
    LOG_PATH.get_or_init(|| std::sync::Mutex::new(None))
}

fn log_format() -> &'static std::sync::Mutex<String> {
    LOG_FORMAT.get_or_init(|| std::sync::Mutex::new("[{level}] {msg}".to_string()))
}

// ==================== THREADING GLOBALS ====================

struct SemaphoreInner {
    max: usize,
    count: std::sync::Mutex<usize>,
    cond: std::sync::Condvar,
}

impl SemaphoreInner {
    fn new(max: usize) -> Self {
        SemaphoreInner { max, count: std::sync::Mutex::new(max), cond: std::sync::Condvar::new() }
    }
    fn acquire(&self) {
        let mut count = self.count.lock().unwrap();
        while *count == 0 {
            count = self.cond.wait(count).unwrap();
        }
        *count -= 1;
    }
    fn try_acquire(&self) -> bool {
        let mut count = self.count.lock().unwrap();
        if *count == 0 { return false; }
        *count -= 1;
        true
    }
    fn release(&self) {
        let mut count = self.count.lock().unwrap();
        *count += 1;
        if *count <= self.max {
            self.cond.notify_one();
        }
    }
}

static THREADING_ID: AtomicU64 = AtomicU64::new(1);
static THREADING_MUTEXES: OnceLock<std::sync::Mutex<HashMap<u64, Arc<std::sync::Mutex<Value>>>>> = OnceLock::new();
static THREADING_SEMAPHORES: OnceLock<std::sync::Mutex<HashMap<u64, Arc<SemaphoreInner>>>> = OnceLock::new();
static THREADING_RWLOCKS: OnceLock<std::sync::Mutex<HashMap<u64, Arc<std::sync::RwLock<Value>>>>> = OnceLock::new();

fn threading_mutexes() -> &'static std::sync::Mutex<HashMap<u64, Arc<std::sync::Mutex<Value>>>> {
    THREADING_MUTEXES.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn threading_semaphores() -> &'static std::sync::Mutex<HashMap<u64, Arc<SemaphoreInner>>> {
    THREADING_SEMAPHORES.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn threading_rwlocks() -> &'static std::sync::Mutex<HashMap<u64, Arc<std::sync::RwLock<Value>>>> {
    THREADING_RWLOCKS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}
