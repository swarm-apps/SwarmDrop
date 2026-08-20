//! 数据目录解析。
//!
//! **命令行宿主的目录与图形界面宿主的目录刻意不同**：CLI 使用独立的设备身份
//! （spec: cli-host「独立的设备身份」），共用目录会让两个进程抢同一份 identity 与
//! 同一个数据库。目录分开是那条规格在文件系统上的兑现，不是巧合。
//!
//! 「数据目录下都有什么」全部收在 [`DataDir`] 的方法里。各处自己拼路径的话，
//! 改一个文件名就要改 N 处，而漏掉的那处会静默地指向一个永远不存在的文件。

use std::path::{Path, PathBuf};

use crate::exit::{CliError, CliResult};

/// 身份与已配对设备文件所在目录（两个文件名由端口层的共享实现决定）。
const DATABASE_FILE: &str = "swarmdrop.db";
/// 本地通道：其余命令经它复用正在运行的节点。**只有 Unix 用得上**（见 [`DataDir::socket`]）。
#[cfg(unix)]
const SOCKET_FILE: &str = "swarmdrop.sock";
/// 单实例仲裁锁。
const LOCK_FILE: &str = "swarmdrop.lock";
/// 设备名等用户配置。
const DEVICE_CONFIG_FILE: &str = "device_config.json";
/// 上次检查更新的时刻。**只是节流状态，不是配置**——删掉它最多让下次启动多查一次网络。
const UPDATE_CHECK_FILE: &str = "update_check.json";

/// 已解析并确保存在的数据目录。
///
/// 构造即保证目录存在——让「目录在不在」只在一个地方回答，后续所有路径使用点
/// 都不必再各自 `create_dir_all`。
#[derive(Debug, Clone)]
pub struct DataDir(PathBuf);

impl DataDir {
    /// 解析数据目录：显式覆盖优先，否则取平台约定位置。
    ///
    /// **取 `data_local_dir` 而非 `data_dir`**：Windows 上后者是 `%APPDATA%`（Roaming），
    /// 会被域漫游配置文件同步到服务器——私钥不该跟着漫游。macOS 与 Linux 上两者解析到
    /// 同一处，所以这不是平台分叉，只是在 Windows 上落到了对的那个。
    pub fn resolve(explicit: Option<PathBuf>) -> CliResult<Self> {
        let dir = match explicit {
            Some(dir) => dir,
            None => directories::ProjectDirs::from("com", "yexiyue", "swarmdrop-cli")
                .ok_or_else(|| {
                    CliError::NodeUnavailable(
                        "无法确定数据目录（HOME 未设置？）；请用 --data-dir 显式指定".into(),
                    )
                })?
                .data_local_dir()
                .to_path_buf(),
        };

        std::fs::create_dir_all(&dir).map_err(|err| {
            CliError::NodeUnavailable(format!("创建数据目录 {} 失败: {err}", dir.display()))
        })?;
        restrict_to_owner(&dir)?;

        Ok(Self(dir))
    }

    /// 目录本身。身份存储取它即可——两个文件名属于端口层共享实现的磁盘格式约定。
    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn database(&self) -> PathBuf {
        self.0.join(DATABASE_FILE)
    }

    /// 本地通道的地址。
    ///
    /// **两个平台的形态不同，这不是可以统一掉的实现细节**：
    ///
    /// | | 形态 | 谁挡住其他用户 |
    /// |---|---|---|
    /// | Unix | 数据目录下的域套接字**文件** | 目录的 0700（见 [`restrict_to_owner`]） |
    /// | Windows | 全局命名空间里的一个管道名 | 管道自己的默认 DACL |
    ///
    /// ⚠️ **Windows 上不能拿数据目录下的路径当通道地址**——命名管道不在文件系统里，
    /// `interprocess` 的 `GenericFilePath` 对非 `\\.\pipe\` 开头的路径直接报
    /// 「not a named pipe path」，于是 `swarmdrop start` 在 Windows 上**整条命令起不来**
    /// （v0.1.0 就是这样）。
    ///
    /// 也**不能改用 `GenericNamespaced` 一把统一**，尽管它正是为跨平台命名设计的：
    /// 它在 Linux 上解析到 abstract namespace、在其余 Unix 上解析到 `/tmp/`，
    /// 两者都不在数据目录里，于是那道 0700 就再也挡不住任何人——而这条通道能启停节点、
    /// 列设备、发文件、应答配对请求。上游把这三种映射逐条写在
    /// `GenericNamespaced` 的文档里，它给的是可移植性，**安全语义得自己选**。
    #[cfg(unix)]
    pub fn socket(&self) -> PathBuf {
        self.0.join(SOCKET_FILE)
    }

    #[cfg(windows)]
    pub fn socket(&self) -> PathBuf {
        named_pipe_path(&self.0)
    }

    /// 清掉通道的陈旧残留。
    ///
    /// **只有 Unix 有东西可清**：域套接字是个文件，不随进程退出消失——留着会让下一个
    /// 进程多走一次「连不上 → 判陈旧」，而监听器还会因「地址已占用」起不来。删除失败
    /// 无所谓，那条路径本来就能自愈。
    ///
    /// Windows 上通道压根不是文件（见 [`Self::socket`]），命名管道也由内核在最后一个
    /// 句柄关闭时回收——**没有可清理的东西，也不能去 `remove_file` 它**。
    ///
    /// 与 [`Self::socket`] 放在同一个类型上，是为了让「通道在这个平台上长什么样」
    /// 只有一个地方需要知道：此前这条知识被拆在两层，`runtime` 那侧也得写一次
    /// `cfg(unix)`——而这次要修的 bug 正是这个形状（有人对着一个不是文件的路径做文件操作）。
    pub fn clear_stale_channel(&self) {
        #[cfg(unix)]
        let _ = std::fs::remove_file(self.socket());
    }

    pub fn lock(&self) -> PathBuf {
        self.0.join(LOCK_FILE)
    }

    pub fn device_config(&self) -> PathBuf {
        self.0.join(DEVICE_CONFIG_FILE)
    }

    /// 启动时更新检查的节流状态。见 [`crate::runtime::update`]。
    pub fn update_check(&self) -> PathBuf {
        self.0.join(UPDATE_CHECK_FILE)
    }
}

/// 把数据目录收紧到「仅属主可访问」。
///
/// 这不是卫生习惯，是**整个数据目录的信任边界**：
///
/// - `identity.json` 自己是 0600（端口层写的），但**本地通道套接字不是**。
///   `create_dir_all` 走 umask，通常落成 0755，于是同机的**其他用户**能连上那条通道——
///   而它能启停节点、列设备、发文件、应答配对请求。私钥保住了，节点却被别人使唤。
/// - 数据库与设备配置同样是明文，也都在这道目录权限之下。
///
/// 用**目录**而不是逐个文件设权限：套接字由 `interprocess` 创建、锁文件由 `File::create`
/// 创建，逐个 chmod 都留着「创建完到 chmod 之间」的窗口，而目录权限在文件出现之前就已就位。
///
/// 边界与本仓既有形态一致（见 `CLAUDE.md`）：防的是「其他用户」，**不防「同用户下的
/// 其他进程」**——那类进程能直接读走 `identity.json` 里的明文私钥并冒充这台设备，
/// 再去拦一条本地通道没有意义。
///
/// ⚠️ **Windows 不做，这是一个已知缺口。** 那里的通道根本不在数据目录里
/// （见 [`DataDir::socket`]），收紧目录管不着它。
///
/// 此处一度写作「`interprocess` 没有暴露那个口子」——**是错的**（2026-08-20 核实）：
/// `interprocess::os::windows::local_socket::ListenerOptionsExt::security_descriptor()`
/// 就是那个口子，配 `SecurityDescriptor::deserialize()` 吃一个 SDDL 字符串。
///
/// 没有顺手补上，是因为**补错的失败形态比缺口更糟**：SDDL 写错时 `deserialize` 返回
/// `Err`，那条路径要么让 `swarmdrop start` 又一次起不来，要么被静默忽略而只是看起来
/// 做了防护。而 `CREATOR OWNER` 这类 SID 在非继承的 DACL 里是否按预期生效、默认 DACL
/// 现在到底放行到什么程度，**本仓一条都没有实测过**，而本机没有 Windows 环境可验
/// （连交叉编译都卡在 `ring` 的 C 代码上）。
///
/// 所以：**不要把「Windows 默认更严」当成结论写进任何地方**，也不要在没有真机验证的
/// 情况下把 SDDL 合进来。
#[cfg(unix)]
fn restrict_to_owner(dir: &Path) -> CliResult<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|err| {
        CliError::NodeUnavailable(format!("收紧数据目录 {} 权限失败: {err}", dir.display()))
    })
}

#[cfg(not(unix))]
fn restrict_to_owner(_dir: &Path) -> CliResult<()> {
    Ok(())
}

/// Windows 上这个数据目录对应的命名管道路径。
///
/// 管道名从数据目录派生：`--data-dir` 允许同机跑多个互不相干的实例，而管道命名空间是
/// **全局**的——名字撞了就等于两个实例互相接管对方的命令。
///
/// ⚠️ **刻意不加 `#[cfg(windows)]`，在所有平台上编译。** `cfg` 掉的代码连语法都不检查，
/// 写错要到 Windows 构建机上才第一次显形——本仓写这段时就踩了一次（raw string 以
/// 反斜杠结尾，是编译错误，而 macOS 上的 `cargo test` 全绿）。放开编译之后，
/// 下面那两条测试在**每个**平台上都跑得到它。
// `not(test)` 那一半不能省：测试里用得到它，那时它并非死代码，而 `expect` 对
// 「没有发生的告警」本身也会告警（`unfulfilled_lint_expectations`）。
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "只有 Windows 用，但要处处编译")
)]
fn named_pipe_path(dir: &Path) -> PathBuf {
    // 规范化能消掉 `.` / `..` / 短名（8.3）等等价写法；失败（目录刚被删？）就退回原路径
    // ——那时最坏结果是多一个通道名，而文件锁仍然是最终仲裁。
    //
    // 大小写一并归一：Windows 的文件系统大小写不敏感，`C:\Users` 与 `c:\users` 是同一个
    // 目录，得到两个名字就等于把一个实例分裂成两个。
    let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let token = fnv1a(&canonical.to_string_lossy().to_lowercase());
    PathBuf::from(format!(r"\\.\pipe\swarmdrop-cli-{token:016x}"))
}

/// FNV-1a（64 位）。
///
/// **不用 `DefaultHasher`**：标准库明写它的算法与种子不保证在版本间不变，而这个值决定
/// 通道名——换一次就等于旧节点还在跑、新命令却连不上它，同时文件锁又不让新进程起节点，
/// 用户陷入「怎么都连不上、也停不掉」。这里要的是一个**写死在代码里**的算法。
///
/// 由 `fnv1a_matches_the_published_vectors` 钉死：算法被换掉时它会红，
/// 而不是等某个用户升级之后发现自己的常驻节点失联。
fn fnv1a(text: &str) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    text.bytes().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_override_is_used_and_created() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("nested").join("data");
        assert!(!target.exists());

        let dir = DataDir::resolve(Some(target.clone())).expect("resolve");

        assert_eq!(dir.path(), target);
        assert!(target.is_dir(), "构造应确保目录存在");
    }

    /// 每个已知文件都落在数据目录内，且互不重名。
    ///
    /// 「都在目录内」防的是某个拼接不慎逃到目录外；「互不重名」防的是两份数据
    /// 共用一个文件而互相覆盖。
    ///
    /// **通道不在这份清单里**：它只有在 Unix 上才是数据目录下的文件（见
    /// [`DataDir::socket`]），由下面那两条按平台各自看守。
    #[test]
    fn known_files_live_inside_the_directory_and_are_distinct() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");

        let paths = [
            dir.database(),
            dir.lock(),
            dir.device_config(),
            dir.update_check(),
        ];
        for path in &paths {
            assert_eq!(path.parent(), Some(dir.path()), "{path:?} 不在数据目录内");
        }

        let unique: std::collections::HashSet<_> = paths.iter().collect();
        assert_eq!(unique.len(), paths.len(), "存在重名文件");
    }

    /// Unix 上通道是数据目录里的一个文件——那道 0700 才管得着它。
    #[cfg(unix)]
    #[test]
    fn the_channel_lives_inside_the_data_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");

        assert_eq!(dir.socket().parent(), Some(dir.path()));
        assert_ne!(dir.socket(), dir.lock());
    }

    /// Windows 的通道名：**必须** `\\.\pipe\` 开头，且与数据目录一一对应。
    ///
    /// 前半条看守的是一个整条命令起不来的失败：`interprocess` 对别的路径直接报
    /// 「not a named pipe path」，`swarmdrop start` 当场退出（v0.1.0 在 Windows 上就是这样）。
    ///
    /// 后半条看守的是一个更隐蔽的：管道命名空间是全局的，两个 `--data-dir` 派生出同一个
    /// 名字，就等于两个本该互不相干的实例互相接管对方的命令；而同一个目录派生出两个名字，
    /// 则会让 `status` 认不出自己刚 `start` 的节点。
    ///
    /// **在每个平台上跑**（不加 `cfg(windows)`），理由见 [`named_pipe_path`]。
    #[test]
    fn the_named_pipe_is_keyed_by_the_data_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let one = tmp.path().join("one");
        let two = tmp.path().join("two");
        std::fs::create_dir_all(&one).expect("create");
        std::fs::create_dir_all(&two).expect("create");

        let name = named_pipe_path(&one);
        let text = name.to_string_lossy();
        // 前缀写全到 `swarmdrop-cli-`，不是啰嗦：raw string **不能以反斜杠结尾**，
        // 只写到 `pipe\` 的那个写法是编译错误。
        assert!(
            text.starts_with(r"\\.\pipe\swarmdrop-cli-"),
            "不是命名管道名: {text}"
        );

        assert_eq!(named_pipe_path(&one), name, "同一个目录必须恒得同一个名字");
        assert_ne!(named_pipe_path(&two), name, "两个目录撞名了");
    }

    /// 通道名的哈希必须是 FNV-1a，**不是「随便一个稳定哈希」**。
    ///
    /// 用官方公布的测试向量钉死。它红了说明有人换掉了算法——那件事在开发机上毫无症状，
    /// 却会让每一个已经在跑常驻节点的 Windows 用户在升级后失联（新命令连不上旧节点，
    /// 文件锁又不让它起新的）。
    #[test]
    fn fnv1a_matches_the_published_vectors() {
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a("foobar"), 0x8594_4171_f739_67e8);
    }

    /// 数据目录必须对其他用户关闭。
    ///
    /// 看守的是本地通道：套接字自己按默认权限创建（`srwxr-xr-x`），拦住其他用户的
    /// 只有这道目录权限。它松掉的表现不是报错，而是同机另一个用户可以直接
    /// `swarmdrop stop` 掉你的节点、列出你的设备、以你的身份发文件。
    #[cfg(unix)]
    #[test]
    fn data_directory_is_closed_to_other_users() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().join("data"))).expect("resolve");

        let mode = std::fs::metadata(dir.path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "数据目录对其他用户可访问: {mode:o}");
    }

    /// 已经存在的目录也要被收紧——用户可能是从旧版本升上来的，
    /// 那时目录是按 umask 建的。
    #[cfg(unix)]
    #[test]
    fn existing_loose_directory_is_tightened() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("legacy");
        std::fs::create_dir_all(&target).expect("create");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        let dir = DataDir::resolve(Some(target)).expect("resolve");

        let mode = std::fs::metadata(dir.path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "存量目录未被收紧: {mode:o}");
    }

    /// 默认目录必须与图形界面宿主区分开——两者共用会让 CLI 抢桌面端的身份与数据库。
    ///
    /// 只断言「路径里带得出 cli 这个限定」，不写死具体位置：那由平台约定决定，
    /// 写死等于把 `directories` 的实现细节钉进测试。
    #[test]
    fn default_directory_is_distinct_from_the_gui_host() {
        let Some(project) = directories::ProjectDirs::from("com", "yexiyue", "swarmdrop-cli")
        else {
            return; // 无 HOME 的环境（少数 CI 沙箱），该场景由 resolve 的错误分支覆盖
        };
        let path = project.data_local_dir().to_string_lossy().to_lowercase();
        assert!(
            path.contains("cli"),
            "默认数据目录未与图形界面宿主区分: {path}"
        );
    }
}
