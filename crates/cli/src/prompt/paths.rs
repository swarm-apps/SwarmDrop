//! 路径这一种回答的输入设施。
//!
//! 交互输入框里**shell 不介入**，于是两件平时由 shell 代劳的事落到了这里：
//!
//! - **把一行拆成若干路径**（[`split_line`]）。用户把几个文件一起拖进终端时，得到的是
//!   一行 shell 转义过的路径——空格写作 `\ `，某些终端还会整条加引号。原样当成一个路径
//!   会得到一个必然不存在的文件名。
//! - **Tab 补全**（[`PathCompletion`]）。没有它，用户只能把一条长路径逐字敲进去；
//!   而改用拖拽又正是上一条要处理的情形。
//!
//! 两者必须**互为逆运算**：补全写回去的字符串要能被 [`split_line`] 原样解回来，
//! 否则补出一个带空格的目录名之后，用户的下一次回车会拿到两个半截路径。
//!
//! ## 为什么不用 `shlex` / `shell-words`
//!
//! 两者都在依赖树里（传递依赖），拿来即用似乎理所当然。**实测三条都不成立**
//! （2026-08-20，shlex 1.3 / shell-words 1.1）：
//!
//! | 输入 | 两库的结果 | 后果 |
//! |---|---|---|
//! | `C:\Users\me\a.txt` | `C:Usersmea.txt` | Windows 上**静默**毁掉路径 |
//! | `"/tmp/My Doc`（引号未闭合） | `None` / `Err` | 补全在最需要它的时候失效 |
//! | 任意 | 只给 `Vec<String>` | 拿不到 token 的起始位置，补全无从原位改写 |
//!
//! 三条各自独立：第一条是因为它们实现的是 POSIX 规则，而那里 `\` 是转义符、在 Windows
//! 却是路径分隔符——而 Windows 终端拖入**不含空格**的文件时给的正是无引号形式；
//! 第二条是因为补全发生在用户敲到一半时，那一刻引号本来就是开着的；第三条是 API 形状。
//!
//! 绕开第三条可以「解析后重建整行」，但那要求解析成功，于是撞回第二条。
//! 所以这里自己写一个**够用的子集**：只认引号与「转义空白/引号」，其余字符原样保留。

use std::path::{MAIN_SEPARATOR, Path, PathBuf};

/// 把一行回答解成若干条路径：拆行 → 去转义 → 展开 `~`。
///
/// **这是本模块唯一的对外入口**（连同单条的 [`parse_one`]）。拆行与 tilde 展开不各自
/// 公开，是因为它们**必须成对使用**：`PathCompletion` 写回去的是转义过的形式，只做其中
/// 一步得到的就是一条带着反斜杠、或没展开 `~` 的路径——而症状只在目录名含空格时才出现。
/// 收成一个入口之后，「只用了一半」在类型层面就写不出来。
pub fn parse(line: &str) -> Vec<PathBuf> {
    split_line(line)
        .iter()
        .map(|raw| expand_tilde(raw))
        .collect()
}

/// 同 [`parse`]，但取第一条——问「一个目录」这类只要一条的场合用。
///
/// 多给了就只认第一条：这个提问的形态本来就只接受一个答案，静默丢掉多余的比报错好
/// （用户下一步会看到它实际用的是哪个）。
pub fn parse_one(line: &str) -> PathBuf {
    parse(line).into_iter().next().unwrap_or_default()
}

/// 展开一条**裸路径**里开头的 `~`。
///
/// ⚠️ **与 [`parse`] 的适用场景严格互斥，别混用**：
///
/// | 来源 | 用哪个 | 为什么 |
/// |---|---|---|
/// | 交互输入框 | [`parse`] / [`parse_one`] | 补全写回的是**转义过的**形式，必须解回来 |
/// | 环境变量、配置值 | 本函数 | 那里的空格就是路径的一部分，**没有任何转义** |
///
/// 混用的代价是静默的：`SWARMDROP_RECEIVE_DIR=/home/me/My Files` 经 `parse` 会被
/// 按未转义空白拆成两条，取第一条得到 `/home/me/My`——于是程序**创建**那个目录并把
/// 收到的文件放进去，而用户在 `My Files` 里怎么找都找不到。环境变量不经 shell，
/// 拿到的永远是裸值（systemd 的 `Environment=` 连引号都会剥掉）。
pub fn expand(raw: &str) -> PathBuf {
    expand_tilde(raw)
}

/// `\` 在这个平台上算不算转义符。
///
/// **Unix 算，Windows 不算**——那里它是路径分隔符，而两件事不能兼得：
///
/// - 若算：目录补全总在末尾补一个 `\`（`C:\dir\images\`），用户接着敲空格再敲下一条
///   路径时，那个 `\` 会把空格吃成字面量，两条路径合成一条无效路径，而报错里出现的
///   文件名用户从没敲过。
/// - 若不算：`C:\Users\me` 这类无引号路径原样保留（**Windows 终端拖入不含空格的文件时
///   给的正是这个形式**）；含空格的路径靠引号表达，而拖拽含空格的文件时终端会自己加引号。
///
/// 用 `cfg!()` 而不是 `#[cfg]`：**两个分支在每个平台上都编译**，[`escape`] 与
/// [`split_line`] 的行为因此都能在任一平台上被测到（`cfg` 掉的代码连语法都不检查）。
const BACKSLASH_ESCAPES: bool = !cfg!(windows);

/// 把用户敲的一行拆成若干路径。
///
/// 三条规则，与 POSIX shell 的**子集**一致（够用即止，不做完整的词法分析）：
///
/// | 写法 | 含义 |
/// |---|---|
/// | 未转义的空白 | 分隔两个路径 |
/// | `'…'` / `"…"` | 整段是一个路径，里面的空白不分隔 |
/// | `\` 后跟空白或引号（**仅 Unix**） | 那个字符是路径的一部分 |
///
/// 最后一条见 [`BACKSLASH_ESCAPES`]。
fn split_line(line: &str) -> Vec<String> {
    tokens(line).0.into_iter().map(|(_, token)| token).collect()
}

/// 拆行，并记下每个路径在原串里的起始字节位置。
///
/// 位置只有补全用得上（它要把补好的那一段写回原位），但**必须与拆分同一份实现**——
/// 两份实现在「引号里的空格」这类地方一旦分歧，补全就会从一个错误的位置开始改写。
fn tokens(line: &str) -> (Vec<(usize, String)>, bool) {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut start = None::<usize>;
    let mut quote = None::<char>;
    let mut chars = line.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        match ch {
            // 转义：只吃掉那些「不转义就会改变含义」的字符，且仅在这个平台把 `\` 当
            // 转义符时（见 `BACKSLASH_ESCAPES`）。
            '\\' if BACKSLASH_ESCAPES
                && matches!(chars.peek(), Some((_, next)) if next.is_whitespace() || *next == '"' || *next == '\'') =>
            {
                let (_, next) = chars.next().expect("peek 过了");
                start.get_or_insert(index);
                current.push(next);
            }
            '\'' | '"' if quote.is_none() => {
                start.get_or_insert(index);
                quote = Some(ch);
            }
            _ if Some(ch) == quote => quote = None,
            _ if ch.is_whitespace() && quote.is_none() => {
                if let Some(at) = start.take() {
                    out.push((at, std::mem::take(&mut current)));
                }
            }
            _ => {
                start.get_or_insert(index);
                current.push(ch);
            }
        }
    }

    // `start.is_some()` **就是**「行尾那个路径还没结束」——扫描器本来就知道这件事。
    // 补全需要的正是它：行尾是未转义空白（或空行）时，用户在开始一个新路径。
    let open = start.is_some();
    if let Some(at) = start {
        out.push((at, current));
    }
    (out, open)
}

/// 把一个路径写成能被 [`split_line`] 原样解回来的形式。
///
/// 两个平台两种写法，与 [`BACKSLASH_ESCAPES`] 严格对应：
///
/// - **Unix**：给空白与引号各加一个 `\`。不做「保险起见多转义几个」——多出来的反斜杠
///   会原样留在字符串里（那条规则只在空白与引号前吃掉它），于是路径就错了。
/// - **Windows**：`\` 不是转义符，所以**整条用双引号包起来**（路径里已有的双引号翻倍，
///   那是 Windows 命令行自己的规则）。不含空白与引号时不加引号，免得屏幕上全是引号。
fn escape(path: &str) -> String {
    if !BACKSLASH_ESCAPES {
        if !path.contains(|ch: char| ch.is_whitespace() || ch == '"') {
            return path.to_owned();
        }
        return format!("\"{}\"", path.replace('"', "\"\""));
    }

    let mut out = String::with_capacity(path.len());
    for ch in path.chars() {
        if ch.is_whitespace() || ch == '"' || ch == '\'' {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// 展开开头的 `~`。
///
/// shell 不介入，所以这件事也得自己做——而用户敲 `~/Downloads/x` 的概率远高于敲绝对路径。
/// **只认开头那一个 `~`**（`~user` 这种查别人的家目录不做：要读 passwd 数据库，
/// 而它对本命令的用途没有意义）。
fn expand_tilde(raw: &str) -> PathBuf {
    let Some(rest) = raw.strip_prefix('~') else {
        return PathBuf::from(raw);
    };
    if !(rest.is_empty() || rest.starts_with(['/', MAIN_SEPARATOR])) {
        return PathBuf::from(raw);
    }
    let Some(home) = directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) else {
        return PathBuf::from(raw);
    };
    // ⚠️ `~` 与 `~/` 必须给出**不同**的结果：后者以分隔符结尾，补全据此判断
    // 「用户已经进到这个目录里了」。`join("")` 正是用来补上那个分隔符的。
    if rest.is_empty() {
        return home;
    }
    home.join(rest.trim_start_matches(['/', MAIN_SEPARATOR]))
}

/// Tab 补全文件路径。
///
/// 只补**最后一个**路径：前面那些已经由空白分隔、是完成态。
pub struct PathCompletion;

impl dialoguer::Completion for PathCompletion {
    fn get(&self, input: &str) -> Option<String> {
        let (mut done, open) = tokens(input);
        // 行尾是空白（或整行为空）⇒ 用户在开始一个新路径，从光标处补。
        let (start, token) = match open.then(|| done.pop()).flatten() {
            Some(pair) => pair,
            None => (input.len(), String::new()),
        };

        let completed = complete(&token)?;
        Some(format!("{}{}", &input[..start], escape(&completed)))
    }
}

/// 把一个写了一半的路径补到「还能确定的最长形式」。
///
/// 匹配到多个时补到它们的公共前缀而不是随便挑一个——那正是 shell 的行为，
/// 用户按下 Tab 之后接着敲一两个字符再按一次即可。
fn complete(token: &str) -> Option<String> {
    // 光秃秃一个 `~`：补成 `~/`，让用户接着往下走。**必须特判**——展开后它是家目录
    // 本身，按「父目录 + 最后一段」拆会把家目录的名字当成待补的前缀，补出一堆
    // 毫不相干的兄弟目录。
    if token == "~" {
        return Some(format!("~{MAIN_SEPARATOR}"));
    }

    let expanded = expand_tilde(token);
    let raw = expanded.to_string_lossy();

    // 以分隔符结尾 ⇒ 用户已经进到这个目录里，要补的是它下面的东西。
    let (dir, prefix) = if raw.is_empty() || raw.ends_with(['/', MAIN_SEPARATOR]) {
        (expanded.as_path(), "")
    } else {
        let parent = expanded.parent().unwrap_or(Path::new(""));
        let name = expanded.file_name().and_then(|n| n.to_str())?;
        (parent, name)
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };

    // 补出来的那一段接在**用户敲的原形式**后面，而不是接在展开后的绝对路径后面：
    // 这样 `~/Down` 补成 `~/Downloads/` 而不是 `/Users/me/Downloads/`，
    // 也不会给一个本来是相对路径的输入平白加上 `./`。
    let base = token.strip_suffix(prefix)?;

    let matched: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            // **隐藏文件只在用户明确敲了 `.` 之后才参与**：否则在家目录里按一次 Tab
            // 会被几十个点文件淹没，而它们几乎不是要发送的对象。
            if !name.starts_with(prefix) || (name.starts_with('.') && !prefix.starts_with('.')) {
                return None;
            }
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            Some(if is_dir {
                format!("{name}{MAIN_SEPARATOR}")
            } else {
                name
            })
        })
        .collect();

    if matched.is_empty() {
        return None;
    }

    // 目录的尾分隔符在上面收集时就带上了，**这里不要再补一次**：唯一命中时
    // `shared` 就是那个带分隔符的名字，再补一个会得到 `images//`。
    let shared = common_prefix(&matched);

    // 没有进展就别改写——原样写回会把光标弹到行尾，用户以为自己敲错了什么。
    (shared.len() > prefix.len()).then(|| format!("{base}{shared}"))
}

/// 一组名字的最长公共前缀（按字符，不按字节——中文文件名很常见）。
fn common_prefix(names: &[String]) -> String {
    let Some((first, rest)) = names.split_first() else {
        return String::new();
    };
    let mut end = first.chars().count();
    for name in rest {
        end = end.min(
            first
                .chars()
                .zip(name.chars())
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    first.chars().take(end).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dialoguer::Completion;

    #[test]
    fn plain_paths_split_on_whitespace() {
        assert_eq!(split_line("a.txt  b.txt"), vec!["a.txt", "b.txt"]);
        assert_eq!(split_line("   "), Vec::<String>::new());
        assert_eq!(split_line(""), Vec::<String>::new());
    }

    /// 拖拽进终端的形态：空格被转义。
    ///
    /// 这是本模块存在的首要理由——原样当成一个路径会得到一个必然不存在的文件名，
    /// 而报错信息会指向那个拼接出来的怪名字，用户根本看不出发生了什么。
    #[test]
    fn dragged_paths_keep_their_escaped_spaces() {
        assert_eq!(
            split_line(r"/Users/me/My\ Files/a.txt"),
            vec!["/Users/me/My Files/a.txt"]
        );
        assert_eq!(
            split_line(r"/tmp/a\ b.txt /tmp/c.txt"),
            vec!["/tmp/a b.txt", "/tmp/c.txt"]
        );
    }

    #[test]
    fn quoted_paths_are_one_piece() {
        assert_eq!(split_line("\"/tmp/a b.txt\""), vec!["/tmp/a b.txt"]);
        assert_eq!(split_line("'/tmp/a b.txt' c"), vec!["/tmp/a b.txt", "c"]);
    }

    /// **Windows 的路径分隔符不得被当成转义符。**
    ///
    /// 按 POSIX 规则解析 `C:\Users\me` 会得到 `C:Usersme`——一个静默错误的路径，
    /// 而错误信息只会说「找不到」。Windows 终端拖入不含空格的文件时给的正是这个形式。
    #[test]
    fn windows_paths_survive_unquoted() {
        assert_eq!(split_line(r"C:\Users\me\a.txt"), vec![r"C:\Users\me\a.txt"]);
    }

    /// **目录补全留下的尾分隔符不得吃掉后面那个空格。**
    ///
    /// 这是 Windows 上一条只在「补全一个目录、接着再敲一条路径」时才显形的缺陷：
    /// 补全总在目录末尾补 `\`，若把它当转义符，紧随其后的空格就成了字面量，
    /// 两条路径合成一条——而报错里出现的文件名用户从没敲过。
    ///
    /// Unix 上同一个位置的 `/` 不是转义符，天然没有这个问题；两边都断言，
    /// 因为规则由 `BACKSLASH_ESCAPES` 按平台切换，而 `cfg` 掉的那半边没人测得到。
    #[test]
    fn a_completed_directory_still_separates_the_next_path() {
        let line = if BACKSLASH_ESCAPES {
            "/dir/images/ b.txt"
        } else {
            r"C:\dir\images\ b.txt"
        };
        assert_eq!(split_line(line).len(), 2, "两条路径被合成了一条: {line}");
    }

    /// 含空格的路径：Unix 用反斜杠转义，Windows 用引号——两种写法都要认得。
    #[test]
    fn spaces_are_understood_in_this_platforms_form() {
        if BACKSLASH_ESCAPES {
            assert_eq!(
                split_line(r"/tmp/My\ Docs/a.txt"),
                vec!["/tmp/My Docs/a.txt"]
            );
        } else {
            assert_eq!(
                split_line("\"C:\\My Docs\\a.txt\""),
                vec![r"C:\My Docs\a.txt"]
            );
        }
    }

    /// 转义与解析必须互为逆运算——否则补出一个带空格的目录名之后，
    /// 用户的下一次回车会拿到两个半截路径。
    #[test]
    fn escaping_round_trips_through_splitting() {
        for original in [
            "/tmp/a b.txt",
            "/tmp/plain.txt",
            r"C:\Users\me\a.txt",
            "/tmp/it's here",
            "/tmp/say \"hi\"",
            // **以分隔符结尾**：目录补全每次都产出这个形状，而它正是那条
            // 「尾分隔符吃掉下一个空格」缺陷的入口。
            "/tmp/dir/",
            r"C:\dir\",
            "/tmp/my dir/",
        ] {
            assert_eq!(
                split_line(&escape(original)),
                vec![original.to_owned()],
                "转义后解不回原样: {original}"
            );
        }
    }

    /// **裸路径里的空格是路径的一部分，不是分隔符。**
    ///
    /// 这条钉住 [`expand`] 与 [`parse`] 的分工。环境变量不经 shell（systemd 的
    /// `Environment=` 连引号都剥掉），`SWARMDROP_RECEIVE_DIR=/home/me/My Files` 拿到的
    /// 就是带空格的裸值——错用 `parse` 会把它截成 `/home/me/My`，于是程序**创建**那个
    /// 目录并把收到的文件放进去，而用户在 `My Files` 里怎么找都找不到。
    #[test]
    fn a_bare_path_keeps_its_spaces() {
        assert_eq!(
            expand("/home/me/My Files"),
            PathBuf::from("/home/me/My Files")
        );
        assert_eq!(
            expand(r"C:\My Docs\Recv"),
            PathBuf::from(r"C:\My Docs\Recv")
        );

        // 对照：同一个串走交互输入那条路会被按 shell 规则拆开——两条路各有各的来源。
        assert_eq!(parse("/home/me/My Files").len(), 2);
    }

    #[test]
    fn tilde_expands_only_at_the_front() {
        let home = directories::UserDirs::new().expect("家目录");
        let home = home.home_dir();
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("~/Downloads"), home.join("Downloads"));
        // 中间的 `~` 与 `~user` 都原样保留。
        assert_eq!(expand_tilde("/tmp/~/x"), PathBuf::from("/tmp/~/x"));
        assert_eq!(expand_tilde("~someone/x"), PathBuf::from("~someone/x"));
    }

    #[test]
    fn common_prefix_counts_characters_not_bytes() {
        let names = vec!["文档一.txt".to_owned(), "文档二.txt".to_owned()];
        assert_eq!(common_prefix(&names), "文档");
        assert_eq!(common_prefix(&[]), "");
    }

    /// 唯一命中补全整段，目录补上尾分隔符（好接着按 Tab 进去）。
    #[test]
    fn a_unique_match_completes_fully() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("report.txt"), b"x").expect("写文件");
        std::fs::create_dir(tmp.path().join("images")).expect("建目录");

        let base = tmp.path().to_string_lossy();
        let completed = PathCompletion
            .get(&format!("{base}/rep"))
            .expect("应当补出来");
        assert_eq!(completed, escape(&format!("{base}/report.txt")));

        let completed = PathCompletion
            .get(&format!("{base}/ima"))
            .expect("应当补出来");
        assert!(
            completed.ends_with(&format!("images{MAIN_SEPARATOR}")),
            "目录要补上尾分隔符: {completed}"
        );
    }

    /// 多个命中只补到公共前缀——**绝不替用户挑一个**，那会静默选中一个他没打算要的文件。
    #[test]
    fn several_matches_stop_at_the_common_prefix() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("report-a.txt"), b"x").expect("写文件");
        std::fs::write(tmp.path().join("report-b.txt"), b"x").expect("写文件");

        let base = tmp.path().to_string_lossy();
        let completed = PathCompletion
            .get(&format!("{base}/rep"))
            .expect("应当补出来");
        assert_eq!(completed, escape(&format!("{base}/report-")));
    }

    /// 只补最后一个路径，前面已完成的那些原样保留（含它们的转义）。
    #[test]
    fn only_the_last_path_is_completed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("report.txt"), b"x").expect("写文件");

        let base = tmp.path().to_string_lossy();
        let line = format!(r"/tmp/a\ b.txt {base}/rep");
        let completed = PathCompletion.get(&line).expect("应当补出来");
        assert!(completed.starts_with(r"/tmp/a\ b.txt "), "{completed}");
        assert!(completed.ends_with(&escape("report.txt")), "{completed}");
    }

    /// 补不出东西时返回 `None`——原样写回会把光标弹到行尾，用户以为自己敲错了什么。
    #[test]
    fn no_match_leaves_the_line_alone() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path().to_string_lossy();
        assert!(PathCompletion.get(&format!("{base}/zzz")).is_none());
    }

    /// 隐藏文件只在用户明确敲了 `.` 之后才参与——否则家目录里按一次 Tab 就被点文件淹没。
    #[test]
    fn dotfiles_need_an_explicit_dot() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join(".secret"), b"x").expect("写文件");

        let base = tmp.path().to_string_lossy();
        assert!(PathCompletion.get(&format!("{base}/")).is_none());
        assert!(PathCompletion.get(&format!("{base}/.sec")).is_some());
    }
}
