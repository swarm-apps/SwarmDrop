//! 缺参数时怎么把目标补出来——**三态骨架只有这一份实现**。
//!
//! 每一条要「目标」的命令都面对同一个三分：
//!
//! | 情形 | 处置 |
//! |---|---|
//! | 参数给了 | 按它定位，定位不到是用法错误 |
//! | 没给，且问得了人 | 列出候选让用户挑 |
//! | 没给，且问不了人 | **立刻以用法错误退出** |
//!
//! 第三态绝不退化成「替用户挑第一条」：这些菜单补的是撤销邀请、解除配对、选发送目标
//! 这类动作的目标，替用户做那个决定没有 undo。
//!
//! 摊在每条命令里各写一遍时，这三分是 6 处近乎一样的 `match`——而漏掉第三态的表现是
//! **在管道与 CI 里永久挂起且日志无异常**（`dialoguer` 会去读一个永不到来的 stdin）。
//! 收在这里之后，命令面只剩一份**声明**：候选集从哪来、菜单每行长什么样、三种措辞
//! 分别说什么。
//!
//! ## 「参数是精确值」那一态不在这里
//!
//! `transfer show <会话标识>` / `inbox show <条目标识>` 的参数是完整 UUID，自己就能定位，
//! **不该为了查一条记录先把几百条列出来**。那条路径因此压根不构造 [`Picker`]——调用点
//! 直接 `match arg { Some(id) => …, None => picker.menu().await? }`。
//!
//! 这不只是省一次取数。它此前是 `Locator::Direct` 变体：为了让 `one()` 有个「行」可还，
//! 那条路要伪造一个**只带标识、其余字段全空**的假行，靠一句注释维持「调用方只读 id」
//! 这个不变量——谁哪天多读一个字段，菜单那条路给出正确值、参数那条路给 `null`，
//! 编译器一声不吭。现在「精确参数不取数」是结构性事实，不是靠测试看守的行为。

use crate::exit::{CliError, CliResult};

use super::{require_can_ask, select, select_many};

/// 把用户敲的参数在候选集里定位成一条记录。
///
/// 用函数指针而非闭包：这些解析器都是自由函数（把领域层的 `resolve_*` 翻成面向用户的
/// 措辞），不需要捕获。要捕获的那个是 [`Picker::label`]。
pub type Locate<T> = fn(&[T], &str) -> CliResult<T>;

/// 一次「选出目标」的完整声明。
///
/// 字段全公开、按结构体字面量构造而不是 builder：五个字段每个都必填，builder 只会
/// 多出五个只调用一次的方法，而字段名本身已经是最好的文档。
pub struct Picker<'a, F, L> {
    /// 取候选集。
    pub fetch: F,
    /// 菜单里的一行。用闭包而非函数指针——它常要捕获上下文（邀请菜单要一个统一的
    /// 「此刻」，否则相邻两行的剩余时间会落在不同的秒上）。
    pub label: L,
    /// 菜单提示语。
    pub prompt: &'a str,
    /// 候选集为空时的措辞。**与 `unavailable` 是两回事**：这里是「没得选」，
    /// 那里是「没法问」，用户的下一步动作完全不同。
    pub empty: &'a str,
    /// 问不了人时的用法错误。要说清「怎么用参数指定」和「去哪看有哪些」。
    pub unavailable: &'a str,
}

// `T` 不是结构体的参数而是这里的：它由 `fetch` 的返回类型唯一确定，写进结构体
// 只会逼每个调用点多标一次类型。
impl<F, L, T> Picker<'_, F, L>
where
    T: Clone,
    F: AsyncFn() -> CliResult<Vec<T>>,
    L: Fn(&T) -> String,
{
    /// 列出候选让用户挑一条。
    ///
    /// 参数缺席时唯一的补法。返回的是**完整的候选行**——调用方要什么字段都在里面，
    /// 不必再查一次（`transfer show` 的清单与详情本来就是同一个类型）。
    pub async fn menu(&self) -> CliResult<T> {
        // **先判能不能问，再取候选集。** 反过来的话，管道与 `--json` 下这条命令会完整
        // 跑一次查询（打开数据库、跑迁移、把整张表读回来，或一次通道往返）然后把结果
        // 丢掉、报一个本可以立刻给出的用法错误——而 spec 要求的正是「立即退出」。
        require_can_ask(self.unavailable)?;
        let rows = self.candidates().await?;

        // 读不到回答 ⇒ 用户中止（Esc / Ctrl-C / 终端没了）。
        let index = select(self.prompt.to_owned(), self.items(&rows))
            .await
            .ok_or(CliError::Aborted)?;
        at(&rows, index)
    }

    /// 参数是模糊标识（邀请标识前缀、设备名）：给了就在候选集里解析，没给就弹菜单。
    ///
    /// 模糊标识**只有放进当前候选集里才谈得上唯一**，所以这条路必须先取候选集——
    /// 与精确参数那条路（不构造 `Picker`，见模块文档）的差别就在这里。
    pub async fn one(&self, arg: Option<&str>, locate: Locate<T>) -> CliResult<T> {
        let Some(arg) = arg else {
            return self.menu().await;
        };
        locate(&self.candidates().await?, arg)
    }

    /// 同 [`Self::one`]，一次多条。
    ///
    /// 给撤销邀请、解除配对这类「一次往往要处理好几条」的动作用：只能单选的话，
    /// 用户得把同一条命令敲 N 遍，每敲一遍都要重新看一次列表、重新认一次标识。
    ///
    /// 其中任一个定位不到就**整条失败**，不部分执行：批量撤销/解除是不可逆的，
    /// 用户敲错一个标识时的正确处置是停下来让他看清楚，而不是撤掉另外那几张之后
    /// 再告诉他有一个没找到。
    pub async fn many(&self, args: &[String], locate: Locate<T>) -> CliResult<Vec<T>> {
        if args.is_empty() {
            return self.menu_many().await;
        }
        let rows = self.candidates().await?;
        args.iter().map(|arg| locate(&rows, arg)).collect()
    }

    /// 列出候选让用户勾若干条。
    ///
    /// **空勾选是中止而不是成功**：勾了零项回车意味着用户看过之后决定不动手，
    /// 那时报告「已撤销 0 张」是在假装做了事。
    async fn menu_many(&self) -> CliResult<Vec<T>> {
        // 顺序同 [`Self::menu`]：问不了人就别先查。
        require_can_ask(self.unavailable)?;
        let rows = self.candidates().await?;

        let picked = select_many(self.prompt.to_owned(), self.items(&rows))
            .await
            .ok_or(CliError::Aborted)?;
        if picked.is_empty() {
            return Err(CliError::Aborted);
        }
        picked.into_iter().map(|index| at(&rows, index)).collect()
    }

    /// 取候选集；空集是用法错误。
    ///
    /// 空集判断收在这里而不是各方法里：它与「问不了人」是两种不同的失败，措辞也不同，
    /// 而两者都必须发生在**动手之前**。
    async fn candidates(&self) -> CliResult<Vec<T>> {
        let rows = (self.fetch)().await?;
        if rows.is_empty() {
            return Err(CliError::Usage(self.empty.into()));
        }
        Ok(rows)
    }

    fn items(&self, rows: &[T]) -> Vec<String> {
        rows.iter().map(&self.label).collect()
    }
}

/// 取出选中的那条。
///
/// 下标由 dialoguer 从我们自己给的 items 派生，越界不可能发生——但这里**不 panic**：
/// 一个交互命令因为内部不变量破了而崩掉，对用户是最难理解的失败形态。
fn at<T: Clone>(rows: &[T], index: usize) -> CliResult<T> {
    rows.get(index)
        .cloned()
        .ok_or_else(|| CliError::Usage("选择超出范围".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit::Code;
    use crate::prompt::{InteractionGuard, no_interaction};

    /// 在候选集里按名字精确定位——替代真实命令里那些前缀 / 重名解析。
    fn locate(rows: &[String], arg: &str) -> CliResult<String> {
        rows.iter()
            .find(|row| row.as_str() == arg)
            .cloned()
            .ok_or_else(|| CliError::Usage(format!("没有 {arg}")))
    }

    /// 造一个候选集固定的 picker。
    fn picker(
        rows: Vec<&'static str>,
    ) -> Picker<'static, impl AsyncFn() -> CliResult<Vec<String>>, impl Fn(&String) -> String> {
        Picker {
            fetch: move || {
                let rows = rows.clone();
                async move { Ok(rows.iter().map(|s| (*s).to_owned()).collect()) }
            },
            label: |row: &String| row.clone(),
            prompt: "选一个",
            empty: "空集",
            unavailable: "请指定",
        }
    }

    /// **给了参数就不问人**——问了就等于在脚本里挂住。
    #[tokio::test]
    async fn an_argument_skips_the_menu() {
        let _guard: InteractionGuard = no_interaction().await;

        let chosen = picker(vec!["甲", "乙"]).one(Some("乙"), locate).await;
        assert_eq!(chosen.expect("应当直接定位"), "乙");
    }

    /// **问不了人时立刻退出，绝不去读 stdin。**
    ///
    /// 这条看守的是最难诊断的一种失败：`dialoguer` 在非 TTY 下会去读一个永不到来的
    /// stdin，表现为**永久挂起且日志无异常**。超时即失败。
    ///
    /// 三个入口逐个钉住——它们各自都会走到 `require_can_ask`，漏掉哪个都是同一个症状。
    #[tokio::test]
    async fn missing_argument_without_a_terminal_fails_fast() {
        let _guard = no_interaction().await;
        let timeout = std::time::Duration::from_secs(5);

        let menu = tokio::time::timeout(timeout, picker(vec!["甲"]).menu())
            .await
            .expect("menu 挂起了——这正是 --no-input 要防的");
        let one = tokio::time::timeout(timeout, picker(vec!["甲"]).one(None, locate))
            .await
            .expect("one 挂起了");
        let many = tokio::time::timeout(timeout, picker(vec!["甲"]).many(&[], locate))
            .await
            .expect("many 挂起了");

        assert_eq!(menu.expect_err("menu 应当报用法错误").code(), Code::Usage);
        assert_eq!(one.expect_err("one 应当报用法错误").code(), Code::Usage);
        assert_eq!(many.expect_err("many 应当报用法错误").code(), Code::Usage);
    }

    /// 候选集为空时用 `empty` 的措辞，**而不是「无法交互」**——用户此刻要做的是
    /// 先让集合非空（去配对、去发一张邀请），不是换个终端再来。
    ///
    /// 直接测 `candidates()`：它是那句措辞的唯一来源。`menu()` 在它之前还有一道
    /// 「问不了人就立刻退出」（那道**刻意排在取数之前**，见 `menu`），而单测环境
    /// 永远不可交互，所以经 `menu()` 测不到这条——两句措辞的优先级本来就是
    /// 「问不了人」在前。
    #[tokio::test]
    async fn an_empty_set_says_so() {
        let err = picker(vec![]).candidates().await.expect_err("空集应当报错");
        assert_eq!(err.code(), Code::Usage);
        assert!(err.to_string().contains("空集"), "{err}");
    }

    /// 而**问不了人时连候选集都不取**——那次查询的结果会被立刻丢掉。
    ///
    /// 用一个「一被调用就 panic 的 fetch」钉住：顺序写反了不报错，只是让管道里
    /// 一条本该立刻失败的命令先跑完一次数据库查询。
    #[tokio::test]
    async fn a_pipe_never_pays_for_the_candidate_set() {
        let _guard = no_interaction().await;

        let picker = Picker {
            fetch: async || -> CliResult<Vec<String>> { panic!("不该取候选集") },
            label: |row: &String| row.clone(),
            prompt: "选一个",
            empty: "空集",
            unavailable: "请指定",
        };

        assert_eq!(
            picker.menu().await.expect_err("应当报用法错误").code(),
            Code::Usage
        );
    }

    /// 多个参数各自定位，顺序按用户给的来。
    #[tokio::test]
    async fn several_arguments_resolve_in_order() {
        let _guard = no_interaction().await;

        let chosen = picker(vec!["甲", "乙", "丙"])
            .many(&["丙".to_owned(), "甲".to_owned()], locate)
            .await;
        assert_eq!(chosen.expect("应当全部定位"), vec!["丙", "甲"]);
    }

    /// 其中一个定位不到 ⇒ **整条命令失败**，不是「跳过它做剩下的」。
    ///
    /// 批量撤销/解除是不可逆的，用户敲错一个标识时的正确处置是停下来让他看清楚，
    /// 而不是撤掉另外那几张之后再告诉他有一个没找到。
    #[tokio::test]
    async fn one_bad_argument_fails_the_whole_batch() {
        let _guard = no_interaction().await;

        let err = picker(vec!["甲", "乙"])
            .many(&["甲".to_owned(), "丁".to_owned()], locate)
            .await
            .expect_err("应当整条失败");
        assert_eq!(err.code(), Code::Usage);
    }
}
