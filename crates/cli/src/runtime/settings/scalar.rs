//! 标量设置：封闭集合、三层来源、写入编排。
//!
//! 「标量」指的是**一个值替换另一个值**的那种设置。集合类的（引导节点）不在这里——
//! 用整值写入表达集合，会迫使调用方先读出当前清单、编辑、再整份写回，那正是
//! `bootstrap-node-settings` 明令禁止的「持久化合并后的最终清单」。
//!
//! ## 三层来源，以及为什么读面必须说出来
//!
//! ```text
//! 环境变量覆盖  →  持久化配置  →  内置默认
//! ```
//!
//! 环境变量保持最高优先级是刻意的：命令行宿主常跑在脚本与服务单元里，那些地方设一个
//! 环境变量比维护一份配置文件自然得多，加了配置文件不等于要把它降级。
//!
//! **难点不在优先级而在告知。** 一个设置界面拿到的如果只是「当前值」，它无法解释为什么
//! 用户刚改的值没生效，用户会反复改那个输入框。所以 [`ScalarView`] 同时给出生效值、
//! 来源、以及被压住的那个持久化值。
//!
//! ## 生效时机对每一项都明确，且都不要求重启节点
//!
//! 另外三端都做得到，而命令行宿主重启节点意味着**断掉正在进行的传输**——一个改名操作
//! 不该有那种代价。所以有节点在跑时两项都即时生效（改名走 core 的改名编排，落点换掉
//! 常驻节点内存里那份），没节点时才是「下次启动生效」。

use serde::{Deserialize, Serialize};

use swarmdrop_core::host::EventBus;
use swarmdrop_host::device::DeviceName;

use crate::exit::{CliError, CliResult};
use crate::runtime::access::Records;
use crate::runtime::boot::{CliNetManager, RunningNode, default_device_name};
use crate::runtime::receive::ReceiveDir;

/// 可配置的标量项。**封闭集合**——消费方按这些标识符寻址，不按帮助文本里的措辞。
///
/// `ValueEnum` 让 clap 自己拒掉不认识的标识符并在错误里列出可用的那些，
/// 于是「静默记下一个没人读的键」这件事没有发生的余地。
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScalarKey {
    /// 本机设备名——对端在设备列表与配对请求里看到的那个。
    DeviceName,
    /// 收到的文件落在哪个目录。
    ReceiveDir,
}

impl ScalarKey {
    /// 命令行上的标识符（与 `--json` 里的 `key` 逐字相同）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeviceName => "device-name",
            Self::ReceiveDir => "receive-dir",
        }
    }

    /// 能压住这一项的环境变量。`None` = 这一项没有环境变量覆盖。
    pub fn env_var(self) -> Option<&'static str> {
        match self {
            Self::DeviceName => None,
            Self::ReceiveDir => Some(crate::adapter::receive::ENV_VAR),
        }
    }
}

/// 一个值此刻从哪来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// 环境变量覆盖。
    Env,
    /// 持久化配置。
    Config,
    /// 内置默认。
    Default,
}

/// 一项配置的完整读面。
///
/// 这不是诊断信息而是契约的一部分：只显示 `value` 的界面，会让用户在环境变量存在时
/// 反复修改一个不生效的输入框，而界面无从解释为什么。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScalarView {
    pub key: ScalarKey,
    /// 当前**生效**的值。
    ///
    /// `None` 只在一种情形下出现：这一项既没被设过，本机又给不出内置默认
    /// （拿不到下载目录的无桌面环境）。此时接收会以一句可行动的错误被拦下，
    /// 而不是把文件收进一个用户找不到的地方。
    pub value: Option<String>,
    pub source: Source,
    /// 持久化配置里的值（没设过则 `None`）。
    ///
    /// `source == Env` 时它就是**那个被压住的值**——界面要显示的正是它，而不是让用户
    /// 对着一个不生效的输入框改来改去。
    pub configured: Option<String>,
    /// 压住持久化配置的环境变量名；`source != Env` 时为 `None`。
    pub overridden_by: Option<String>,
}

/// 一次写入此刻算不算数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Effect {
    /// 已经生效，不必重启节点。
    Applied,
    /// 已保存，下次节点启动时生效（此刻没有节点在跑）。
    PendingStart,
    /// 已保存，但当前被环境变量压着，此刻不生效。
    #[serde(rename_all = "camelCase")]
    Overridden { by: String },
}

/// 一次写入的结果。
///
/// 带上写完之后的完整读面，而不只是「已保存」：调用方要据此决定要不要提示用户
/// （「你改的这个值现在还不算数」），只有一个成功位是判断不出来的。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScalarWritten {
    #[serde(flatten)]
    pub view: ScalarView,
    pub effect: Effect,
}

/// 写入时能用上的、正在运行的节点。
///
/// `None`（不构造它）表示没有节点在跑：那时写入只落盘，**绝不为了写入而拉起一个节点**
/// （spec: `cli-command-surface` 的「只有需要联网的命令才启动节点」）。
pub struct Live<'a> {
    pub node: &'a RunningNode,
    /// 常驻节点内存里的接收落点。改落点要连它一起换，否则盘上与内存分叉——
    /// 用户改完之后收到的文件仍然落在旧目录，重启之后才「莫名其妙地」搬家。
    pub receive: &'a ReceiveDir,
}

/// 读出全部标量项的当前状态。
///
/// **不碰文件系统**：读一次配置不该顺手把目录建出来。目录的建立发生在真正要往里写东西
/// 的时候（节点启动、以及 [`apply`] 的校验）。
pub async fn views(records: &Records) -> CliResult<Vec<ScalarView>> {
    let stored = records.settings().read()?;
    let saved_name = {
        use swarmdrop_core::host::DeviceConfig;
        records.device_config().load_device_name().await
    };

    Ok(vec![
        device_name_view(saved_name),
        receive_dir_view(stored.receive_dir),
    ])
}

/// 从全量读面里挑一条。
///
/// **`config get` 与写入回报共用它**，而不是各自另写一条取数路径：两条路径迟早会对同一项
/// 给出不同的来源判定，而那正是这个读面存在的理由。
///
/// 挑不到只可能是本模块自己不一致（封闭集合里的每一项都必然在 [`views`] 里），
/// 由 `the_read_face_covers_every_key` 看守。
pub fn pick(views: Vec<ScalarView>, key: ScalarKey) -> CliResult<ScalarView> {
    views
        .into_iter()
        .find(|view| view.key == key)
        .ok_or_else(|| CliError::NodeUnavailable(format!("配置项 {} 没有读面", key.as_str())))
}

/// 单项读面。
pub async fn view(records: &Records, key: ScalarKey) -> CliResult<ScalarView> {
    pick(views(records).await?, key)
}

fn device_name_view(saved: Option<DeviceName>) -> ScalarView {
    let configured = saved.map(DeviceName::into_string);
    let source = if configured.is_some() {
        Source::Config
    } else {
        Source::Default
    };
    ScalarView {
        key: ScalarKey::DeviceName,
        value: configured
            .clone()
            .or_else(|| default_device_name().map(DeviceName::into_string)),
        source,
        configured,
        overridden_by: None,
    }
}

/// 接收落点的三层判定。
///
/// `pub(crate)` 是给 [`crate::runtime::receive::ReceiveDir`] 用的——接收路径与配置读面
/// **必须共用同一份判定**，否则会出现「`config list` 说落点是 A，文件却收进了 B」。
pub(crate) fn receive_dir_view(configured: Option<String>) -> ScalarView {
    tiers(
        crate::adapter::receive::from_env(),
        configured,
        crate::adapter::receive::default_dir(),
    )
}

/// 三层择一的**纯**判定。
///
/// 与门面分开是为了让它测得动：环境变量是进程级的，在并行跑的测试里改它会互相踩，
/// 于是那条最要紧的规则（环境变量压住配置、且被压住的值仍要给出来）反而成了唯一
/// 测不到的一条。
fn tiers(
    from_env: Option<std::path::PathBuf>,
    configured: Option<String>,
    default: Option<std::path::PathBuf>,
) -> ScalarView {
    let display = |path: std::path::PathBuf| path.to_string_lossy().into_owned();

    let (value, source) = match (from_env, &configured) {
        (Some(env), _) => (Some(display(env)), Source::Env),
        (None, Some(dir)) => (Some(dir.clone()), Source::Config),
        (None, None) => (default.map(display), Source::Default),
    };

    ScalarView {
        key: ScalarKey::ReceiveDir,
        value,
        overridden_by: matches!(source, Source::Env)
            .then(|| ScalarKey::ReceiveDir.env_var().map(str::to_owned))
            .flatten(),
        source,
        configured,
    }
}

/// 写入一个标量项。`value = None` 表示清除，使它回落到下一层来源。
///
/// 校验在落盘之前，且**零网络往返**：设备名走与其余三端同一个归一化入口，落点整串处理
/// 并当场验证可建可写。
pub async fn apply(
    records: &Records,
    live: Option<Live<'_>>,
    key: ScalarKey,
    value: Option<String>,
) -> CliResult<ScalarWritten> {
    match key {
        ScalarKey::DeviceName => write_device_name(records, live.as_ref(), value).await,
        ScalarKey::ReceiveDir => write_receive_dir(records, live.as_ref(), value).await,
    }?;

    let view = view(records, key).await?;
    let effect = effect_of(&view, live.is_some());
    Ok(ScalarWritten { view, effect })
}

/// 这次写入此刻算不算数。
///
/// **环境变量的判断在最前**：它同时管「有节点」与「没节点」两种情形，而它是三者里唯一
/// 一个用户不改环境就永远不会生效的。
fn effect_of(view: &ScalarView, live: bool) -> Effect {
    match &view.overridden_by {
        Some(by) => Effect::Overridden { by: by.clone() },
        None if live => Effect::Applied,
        None => Effect::PendingStart,
    }
}

/// 改名：走 core 的改名编排，**不自己拼 `agent_version`**。
///
/// 那条编排把「落盘 → 本机 `OsInfo` → identify 的 `agent_version` → 事件」四步的顺序
/// 与失败语义定死了（持久化在最前，因为「名字自己回滚」是最难向用户解释的状态），
/// 宿主自行拼装等于把那份顺序再猜一遍。节点没起时它自己走「只落盘」分支。
async fn write_device_name(
    records: &Records,
    live: Option<&Live<'_>>,
    value: Option<String>,
) -> CliResult<()> {
    let name = match value {
        Some(raw) => Some(DeviceName::parse(&raw).ok_or_else(|| {
            CliError::Usage(format!(
                "设备名不能为空或只有空白；最长 {} 个字符。\n\
                 要清除它（回落到本机主机名）请用 swarmdrop config unset device-name。",
                DeviceName::MAX_CHARS
            ))
        })?),
        None => None,
    };

    let device_config = records.device_config();
    let result = match live {
        Some(live) => {
            swarmdrop_core::device_name::rename_device(
                name,
                &device_config,
                live.node.events.as_ref() as &dyn EventBus,
                Some(&live.node.manager),
            )
            .await
        }
        // 没有节点 ⇒ core 走「只落盘」分支。事件仍然发，但本进程里没有订阅者——
        // 一次性命令的事件总线本来就只服务它自己那条命令。
        None => {
            let bus = crate::adapter::events::CliEventBus::for_mode(true);
            swarmdrop_core::device_name::rename_device(
                name,
                &device_config,
                &bus,
                None::<&CliNetManager>,
            )
            .await
        }
    };

    result.map_err(|err| CliError::NodeUnavailable(format!("保存设备名失败: {err}")))
}

/// 换落点：校验 → 落盘 → 换掉常驻节点内存里那份。
///
/// **只对此后收下的内容生效**，已经落盘的文件留在原处不动：一次传输的文件散在两个目录
/// 比「旧文件还在旧位置」更难解释。
async fn write_receive_dir(
    records: &Records,
    live: Option<&Live<'_>>,
    value: Option<String>,
) -> CliResult<()> {
    let configured = match value {
        Some(raw) if raw.trim().is_empty() => {
            return Err(CliError::Usage(
                "接收落点不能是空串。要清除它请用 swarmdrop config unset receive-dir。".into(),
            ));
        }
        Some(raw) => {
            // ⚠️ **整串处理**：`~` 要展开（命令行上 shell 已经展开过，但经通道过来的、
            // 以及配置文件里的都没有），而空格**不是**分隔符——`/home/me/My Files`
            // 被按 shell 规则截成 `/home/me/My` 的失败形态是静默的，用户只会发现
            // 文件不见了。
            let dir = crate::prompt::paths::expand(raw.trim());
            // 当场验证可建可写。挡不住的只有「以后被别人改成只读」，那由接收时的错误兜底。
            let dir = crate::adapter::receive::ensure_writable(dir)?;
            Some(dir.to_string_lossy().into_owned())
        }
        None => None,
    };

    let stored = records.settings().update(|settings| {
        settings.receive_dir = configured;
        Ok(())
    })?;

    // 常驻节点内存里那份跟着换。**按三层来源重算而不是直接用刚写的值**：环境变量还压着
    // 的时候，这次写入只是「对未来的声明」，此刻的落点一个字都不该动。
    if let Some(live) = live {
        live.receive.refresh(&stored)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个空数据目录上的 [`Records`]。返回的 `TempDir` 是 RAII guard，
    /// 调用点必须把它绑到一个活到用例结束的名字上。
    fn fixture() -> (tempfile::TempDir, Records) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = crate::adapter::paths::DataDir::resolve(Some(tmp.path().to_path_buf()))
            .expect("resolve");
        (tmp, Records::new(dir))
    }

    /// 命令行上的标识符与 clap 认的那个必须逐字相同——不同的话，`--help` 列出来的键
    /// 与 `--json` 里回来的键会对不上，而消费方是按后者寻址的。
    ///
    /// 封闭集合从 clap 的 `value_variants()` 取，**不另立一份常量清单**：两份的话，
    /// 新增一项而漏改其中一份是静默的。
    #[test]
    fn every_key_agrees_with_its_command_line_spelling() {
        use clap::ValueEnum;
        for key in ScalarKey::value_variants() {
            let spelled = key.to_possible_value().expect("每个键都要能出现在命令行上");
            assert_eq!(spelled.get_name(), key.as_str(), "{key:?} 的拼写不一致");
        }
    }

    /// **读面必须覆盖封闭集合里的每一项。**
    ///
    /// 漏掉一项是静默的：`config get` 会以一句「没有读面」失败，而那句话指向的是本模块
    /// 的内部不一致，用户无从下手。
    #[tokio::test]
    async fn the_read_face_covers_every_key() {
        use clap::ValueEnum;

        let (_tmp, records) = fixture();

        let covered: Vec<ScalarKey> = views(&records)
            .await
            .expect("读面")
            .into_iter()
            .map(|view| view.key)
            .collect();
        assert_eq!(covered, ScalarKey::value_variants(), "读面漏了配置项");
    }

    /// 环境变量存在时：生效值来自它，而**被压住的那个持久化值仍要给出来**。
    ///
    /// 少了 `configured` 那一半，设置界面只能显示一个用户改不动的值，
    /// 于是用户会反复修改一个不生效的输入框。
    #[test]
    fn an_overridden_value_still_reports_what_was_configured() {
        // 直接测纯函数，不动进程级环境变量（并行测试下会互相踩）。
        let view = ScalarView {
            key: ScalarKey::ReceiveDir,
            value: Some("/from/env".into()),
            source: Source::Env,
            configured: Some("/from/config".into()),
            overridden_by: ScalarKey::ReceiveDir.env_var().map(str::to_owned),
        };
        assert_eq!(view.configured.as_deref(), Some("/from/config"));
        assert_eq!(
            effect_of(&view, true),
            Effect::Overridden {
                by: "SWARMDROP_RECEIVE_DIR".to_owned()
            },
            "有节点在跑也不该报「已生效」——环境变量还压着"
        );
    }

    /// 没有覆盖时，生效状态只由「有没有节点」决定。
    #[test]
    fn effect_follows_whether_a_node_is_running() {
        let view = ScalarView {
            key: ScalarKey::DeviceName,
            value: Some("书房".into()),
            source: Source::Config,
            configured: Some("书房".into()),
            overridden_by: None,
        };
        assert_eq!(effect_of(&view, true), Effect::Applied);
        assert_eq!(effect_of(&view, false), Effect::PendingStart);
    }

    /// 没设过的项要给出**内置默认**并标明来源，而不是空值。
    #[test]
    fn an_unset_value_falls_back_to_the_builtin_default() {
        let view = device_name_view(None);
        assert_eq!(view.source, Source::Default);
        assert!(view.configured.is_none());
        assert!(
            view.value.is_some(),
            "设备名恒有内置默认（本机名 + (cli) 后缀）"
        );

        let saved = DeviceName::parse("书房 Mac").expect("非空");
        let view = device_name_view(Some(saved));
        assert_eq!(view.source, Source::Config);
        assert_eq!(view.value.as_deref(), Some("书房 Mac"));
    }

    /// **三层的优先级：环境变量 → 配置 → 内置默认。**
    ///
    /// 三条一起断言而不是各写一条，是因为要看的正是它们的**相对**次序。
    #[test]
    fn the_three_tiers_are_tried_in_order() {
        let env = Some(std::path::PathBuf::from("/from/env"));
        let default = Some(std::path::PathBuf::from("/from/default"));

        let overridden = tiers(env.clone(), Some("/from/config".into()), default.clone());
        assert_eq!(overridden.source, Source::Env);
        assert_eq!(overridden.value.as_deref(), Some("/from/env"));
        // 被压住的那个值必须给出来——界面要显示的是它。
        assert_eq!(overridden.configured.as_deref(), Some("/from/config"));
        assert_eq!(
            overridden.overridden_by.as_deref(),
            Some("SWARMDROP_RECEIVE_DIR")
        );

        let configured = tiers(None, Some("/from/config".into()), default.clone());
        assert_eq!(configured.source, Source::Config);
        assert_eq!(configured.value.as_deref(), Some("/from/config"));
        assert!(configured.overridden_by.is_none());

        let fallback = tiers(None, None, default);
        assert_eq!(fallback.source, Source::Default);
        assert_eq!(fallback.value.as_deref(), Some("/from/default"));
        assert!(fallback.configured.is_none());
    }

    /// 环境变量存在但配置从未设过：`configured` 是空，而 `value` 仍然有——
    /// 这两半不能混（「没设过」不等于「设的是空」）。
    #[test]
    fn an_override_without_a_configured_value_reports_no_configured_value() {
        let view = tiers(Some("/from/env".into()), None, None);
        assert_eq!(view.source, Source::Env);
        assert!(view.configured.is_none());
        assert_eq!(view.value.as_deref(), Some("/from/env"));
    }

    /// 本机给不出下载目录且用户没设过：`value` 为空**且来源仍是内置默认**。
    ///
    /// 不能报错——`config list` 在无桌面环境里照样要能列出来，用户正是要靠它发现
    /// 「这一项得自己设」。
    #[test]
    fn a_platform_without_a_download_dir_reports_an_absent_value() {
        let view = tiers(None, None, None);
        assert_eq!(view.source, Source::Default);
        assert!(view.value.is_none());
    }

    /// 一次完整的写入：落盘 → 读回来是「配置」这一档 → 没有节点时报「待下次启动」。
    #[tokio::test]
    async fn writing_without_a_node_persists_and_reports_pending() {
        let (_tmp, records) = fixture();

        let written = apply(
            &records,
            None,
            ScalarKey::DeviceName,
            Some("书房 Mac".into()),
        )
        .await
        .expect("写入");

        assert_eq!(written.effect, Effect::PendingStart);
        assert_eq!(written.view.source, Source::Config);
        assert_eq!(written.view.value.as_deref(), Some("书房 Mac"));

        // 读回来是同一个值——写入路径与读面用的是同一份持久化。
        let back = view(&records, ScalarKey::DeviceName).await.expect("读回");
        assert_eq!(back.configured.as_deref(), Some("书房 Mac"));
    }

    /// **清除必须回落到默认值，而不是变成空值。**
    #[tokio::test]
    async fn clearing_falls_back_to_the_default_not_to_an_empty_value() {
        let (_tmp, records) = fixture();

        apply(&records, None, ScalarKey::DeviceName, Some("书房".into()))
            .await
            .expect("先设一个");
        let cleared = apply(&records, None, ScalarKey::DeviceName, None)
            .await
            .expect("清除");

        assert_eq!(cleared.view.source, Source::Default);
        assert!(cleared.view.configured.is_none());
        assert!(
            cleared.view.value.is_some(),
            "清除之后仍要有一个生效值（本机名 + (cli) 后缀）"
        );
    }

    /// 空白设备名是用法错误，**不是**「清除」——后者有自己的动作。
    #[tokio::test]
    async fn a_blank_device_name_is_refused() {
        let (_tmp, records) = fixture();
        let err = apply(&records, None, ScalarKey::DeviceName, Some("   ".into()))
            .await
            .expect_err("空白名必须被拒");
        assert_eq!(err.code(), crate::exit::Code::Usage);
    }

    /// **含空格的落点整串处理，不按 shell 规则拆。**
    ///
    /// 拆了的失败形态是静默的：`/tmp/…/My Files` 被截成 `/tmp/…/My`，
    /// 用户只会发现文件不见了。
    #[tokio::test]
    async fn a_receive_dir_with_spaces_is_kept_whole() {
        let (tmp, records) = fixture();
        let target = tmp.path().join("My Files");

        let written = apply(
            &records,
            None,
            ScalarKey::ReceiveDir,
            Some(target.to_string_lossy().into_owned()),
        )
        .await
        .expect("写入");

        assert_eq!(
            written.view.configured.as_deref(),
            Some(target.to_string_lossy().as_ref())
        );
        assert!(target.is_dir(), "落点应当被创建出来");
    }

    /// 空串不是「清除」——清除有自己的动作，把空串收下会留下一个谁也用不了的落点。
    #[tokio::test]
    async fn an_empty_receive_dir_is_refused() {
        let (_tmp, records) = fixture();
        let err = apply(&records, None, ScalarKey::ReceiveDir, Some("  ".into()))
            .await
            .expect_err("空串必须被拒");
        assert_eq!(err.code(), crate::exit::Code::Usage);
    }

    /// 结构化读面的字段名是契约的一部分——消费方按它们寻址。
    #[test]
    fn the_structured_shape_is_stable() {
        let written = ScalarWritten {
            view: ScalarView {
                key: ScalarKey::ReceiveDir,
                value: Some("/tmp/drop".into()),
                source: Source::Config,
                configured: Some("/tmp/drop".into()),
                overridden_by: None,
            },
            effect: Effect::PendingStart,
        };
        let json = serde_json::to_value(&written).expect("序列化");
        assert_eq!(json["key"], "receive-dir");
        assert_eq!(json["value"], "/tmp/drop");
        assert_eq!(json["source"], "config");
        assert_eq!(json["effect"]["kind"], "pendingStart");
    }
}
