# 一个有 setter 没 reader 的开关：rtc#137

> 本系列第 2 篇。前置：[01 libp2p 的两种 WebRTC](01-libp2p-webrtc-direct.md)。
>
> 上游 PR：[webrtc-rs/rtc#137](https://github.com/webrtc-rs/rtc/pull/137)

## 症状：direct 模式的服务端根本起不来

上一篇讲过，direct 模式的服务端必须关掉 DTLS 指纹校验——它给客户端合成的 offer 里填的是
占位指纹 `FF:FF:…:FF`，客户端出示的真证书永远匹配不上。

rtc 0.20 有这个开关，名字一眼就对：

```rust
setting_engine.disable_certificate_fingerprint_verification(true);
```

设了。然后 DTLS 握手照样失败：

```text
ErrNoMatchingCertificateFingerprint
```

一个布尔开关设了不生效，第一反应总是「我用错了」——是不是设晚了？是不是要在
`RTCPeerConnection` 建之前？是不是还有第二个开关要一起开？

这类怀疑很难自证清白，因为**你无法从外部区分「开关无效」和「开关生效了但还有别的问题」**。
唯一的出路是读源码。

## 定位：字段有，setter 有，reader 没有

在 rtc 仓库里全局搜这个名字，一共只有三处命中：

```text
src/peer_connection/configuration/setting_engine.rs:339   pub(crate) disable_certificate_fingerprint_verification: bool,
src/peer_connection/configuration/setting_engine.rs:775   pub fn disable_certificate_fingerprint_verification(&mut self, is_disabled: bool) {
src/peer_connection/configuration/setting_engine.rs:776       self.disable_certificate_fingerprint_verification = is_disabled;
```

**字段定义、setter、setter 里的赋值。没有任何一处读它。**

值写进了结构体，然后就躺在那里，谁也不去看。这个开关是**死代码**——不是「行为不符合预期」，
是它压根没有行为。

### 旁证：隔壁那个开关接线是完整的

光凭「搜不到 reader」下结论有点虚（可能经宏、经反射读了）。但同一个结构体里，旁边站着一个
形态一模一样的开关：

```rust
// src/peer_connection/internal.rs —— 修复前
RTCDtlsTransport::new(
    certificates,
    setting_engine.answering_dtls_role,
    setting_engine.srtp_protection_profiles.clone(),
    setting_engine.allow_insecure_verification_algorithm,  // ← 这个传下去了
    setting_engine.replay_protection,                      // ← 这个也传下去了
)?;                                                        // ← disable_… 从没出现
```

`allow_insecure_verification_algorithm` 走的是**完全相同**的路径：
`SettingEngine` → `internal.rs` → `RTCDtlsTransport::new` → `ConfigBuilder`。它接得好好的。

两个并排一看，答案就很清楚了：这是 sans-io 重构时**漏掉的一环**，不是什么深层设计。

> **定位技巧**：找一个「本该长得一样」的邻居做对照。孤立地看一段代码，很难判断它是
> 漏了还是故意的；有个正确的同类摆在旁边，缺口立刻显形。

## 第一版修复，以及它埋的雷

补上那一环，然后在建 DTLS 配置的地方按开关决定要不要装校验回调：

```rust
// 第一版（后来被评审否掉）
.with_verify_peer_certificate(
    if self.disable_certificate_fingerprint_verification {
        None                              // 不装回调
    } else {
        Some(verify_peer_certificate)     // 装上
    },
)
```

看起来天经地义：不校验，那就不装校验回调。跑通了，direct 服务端起来了，
浏览器拨得上了，测试全绿，提 PR。

**这一版本身没有错。** 但它后面引出的评审往返，比这个 bug 本身更值得记——
因为我在那轮里犯了一个更典型的错误。

## 测试怎么写：负向那条才是关键

先说测试，因为它和后面的评审有关。

这个 PR 的测试写了**两条，方向相反**：

```rust
// tests/dtls_disable_fingerprint_verification.rs
test disabled_verification_accepts_mismatched_fingerprint   // 开了开关 → 指纹不匹配也能连上
test default_verification_rejects_mismatched_fingerprint    // 不开开关 → 同样的配置仍然失败
```

两条都用同一套 setup：把 answerer 的 SDP 里 `a=fingerprint:` 换成一个匹配不了任何证书的
占位值，然后看能不能连上。

**为什么必须有第二条。** 只写正向测试的话，它在「开关又被忽略了」的世界里**照样通过**——
因为开关被忽略时，代码路径退回默认行为，而如果默认行为恰好也能连上（比如某天有人把
`insecure_skip_verify` 改成绕过一切），正向测试根本发现不了。

负向测试锁住的是「默认仍然严格」。有它在，正向测试才有意义：**两条一起才构成
「这个开关确实在起作用」的证据**。

> 这个模式后面还会再出现：[第 06 篇](06-remote-fingerprint-via-stats.md) 的测试用的是
> 交叉验证（一端的 remote 必须等于另一端的 local），本质是同一件事——
> **让「实现写反了」这个具体的失败模式无处藏身**。

## 评审第一轮：一句看不懂的话

上游维护者 rainliu 给了 `CHANGES_REQUESTED`，评论只有一句：

> why not move this to line 1135?

评论锚在那个 `if` 上。但**这个文件总共只有 232 行**——没有 1135 行。

看起来是多打了一个 1。第 135 行是什么？正是那个校验回调的定义处：

```rust
// src/peer_connection/transport/dtls/mod.rs:135
let verify_peer_certificate: VerifyPeerCertificateFn = Arc::new(
    move |certs: &[Vec<u8>], _chains: &[CertificateDer<'static>]| -> Result<()> {
        if certs.is_empty() {
            return Err(Error::ErrNonCertificate);
        }
        // ...逐个比对指纹...
        Err(Error::ErrNoMatchingCertificateFingerprint)
    },
);
```

于是我推断：他要我把判断挪进**闭包内部**——不在调用点用 `Option` 切换，而是在闭包里早退。

改的时候我自以为还发现了一件事：

> 传 `None` 会跳过**整个回调**，连它自己那句 `certs.is_empty()` → `ErrNonCertificate`
> 一起丢掉。「跳过指纹比对」和「接受一个不出示证书的对端」是两件事。

听起来很有道理，我把它写进了回复。**但这个推论是错的。** 下面会说到。

## 评审第二轮：他其实是另一个意思

rainliu 直接贴了代码，一步到位：

```rust
let verify_peer_certificate: Option<VerifyPeerCertificateFn> =
    if !self.disable_certificate_fingerprint_verification {
        Some(Arc::new(...))
    } else {
        None
    };
```

以及调用点 `.with_verify_peer_certificate(verify_peer_certificate)`。

他要的不是「闭包内早退」，而是**在构造处就决定要不要构造这个闭包**——语义上和我最初那版
（传 `None`）完全一样，只是把三元判断从调用点挪到了变量声明处，读起来更顺，也不会白白
分配一个永远不会被调用的 `Arc<闭包>`。

**我的推断错了。** 那句「135」指的是构造处，不是闭包体内。

## 那个「语义 bug」并不存在

既然他坚持传 `None`，我就得回去验证自己上一轮的论断到底成不成立——传 `None`
真的会放行「不出示证书的对端」吗？

不会。`RTCDtlsTransport` 配的是 `ClientAuthType::RequireAnyClientCert`，而 DTLS 层
自己就在管这件事：

```rust
// rtc-dtls/src/flight/flight4.rs
ClientAuthType::RequireAnyClientCert => {
    if state.peer_certificates.is_empty() {
        return Err((
            Some(Alert { alert_level: Fatal, alert_description: NoCertificate }),
            Some(Error::ErrClientCertificateRequired),
        ));
    }
}
```

而且拦在回调**之前**；`CertificateVerify` 那条路径上还有第二道
（`ErrCertificateVerifyNoCertificate`）。

所以闭包里那句 `certs.is_empty()` 在这条路径上是**冗余防御**，去掉它什么也不会发生。
我的第一版没有安全问题，他的写法也没有——**只有我那条「发现了语义 bug」的断言是错的**。

按他的形状改完，两个集成测试原样通过，包括那条负向的（它正好证明了「默认仍然严格」
在新写法下依然成立）。回复里我明确纠正了上一轮的论断，附上了 flight4 的代码位置。

## 教训

**1. 一个布尔开关不生效，先怀疑它是死的。**
读源码时不要只搜「它在哪被设置」，要搜「它在哪被**读取**」。字段 + setter + 赋值三处命中、
零处读取，就是死代码的标准形状。

**2. 找一个本该长得一样的邻居做对照。**
`allow_insecure_verification_algorithm` 走同一条路径且接线完整，两个并排一看，缺口自己
显形。孤立地读一段代码很难判断「是漏了还是故意的」。

**3. 「关掉一个检查，到底关掉了几件事」——问题是对的，答案要去代码里找。**
我问对了这个问题：「不校验指纹」和「不要求证书」确实是两件事，值得分开确认。
但我**推理**出了答案，而不是**验证**出答案——真相在 `flight4.rs` 里躺着，
grep 一次就能看到。**问对问题只是一半，另一半是去读那段代码。**

**4. 评审里看不懂的意见，先按最合理的解读做一遍——但要把推断本身写进回复。**
`1135` 显然是笔误，与其等一轮问答，不如按最可能的解读改完。这次我推断错了，
代价只是再改一次（几个小时），仍然比空等一轮便宜。

更关键的是：**因为我在回复里写明了「我把 1135 读成了 135，也就是闭包定义处」，
他一眼就看出我理解偏了，直接贴出了他想要的代码。** 如果我只是默默改完说「已修复」，
这一轮还得再来一次。**推断可以错，但推断必须是公开的。**

**5. 自以为拿得准的技术断言，也要先跑一遍。**
这条见 [第 07 篇](07-upstream-methodology.md)——同一个系列里我栽了两次，
一次是 semver 规则，一次就是这里。

---

**上一篇**：[libp2p 的两种 WebRTC](01-libp2p-webrtc-direct.md) ·
**下一篇**：[`send` 返回 `Ok(())`，数据却蒸发了](03-datachannel-silent-send.md)

**上游**：[rtc#137](https://github.com/webrtc-rs/rtc/pull/137) ·
**本仓**：这条补丁已随 rtc 0.20.0 正式版发布，pin 于 2026-08-04 删除；
`crates/webrtc-p2p/Cargo.toml` 现在记录的是**版本下限**及其理由

---

**上一篇**：[libp2p 的两种 WebRTC](01-libp2p-webrtc-direct.md) ·
**下一篇**：[`send` 返回 `Ok(())`，数据却蒸发了](03-datachannel-silent-send.md)

**上游**：[rtc#137](https://github.com/webrtc-rs/rtc/pull/137) ·
**本仓**：这条补丁已随 rtc 0.20.0 正式版发布，pin 于 2026-08-04 删除；
`crates/webrtc-p2p/Cargo.toml` 现在记录的是**版本下限**及其理由
