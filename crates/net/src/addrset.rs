//! 地址集合的规范形式与派生。
//!
//! 内核里有三处在做同一件事（可拨地址 = 监听 ∪ 外部、外部地址视图 = 声明 ∪ 自动确认、
//! 单份声明自身的去重），语义完全相同，故收在这里一份。
//!
//! **为什么放 `crates/net` 而不是 `net-base`。** net-base 是**类型**底座（libp2p 类型在
//! 那里收口成 newtype），它导出的每一项都是类型；而这是个集合算法，且它编码的那条不变量
//! ——「结果必须确定性，否则 watch 的变更检测恒为真、白广播一轮」——是**本 crate** 的
//! 不变量：net-base 里既没有 watch 也没有广播，那条判据在那儿无处安放。

use swarmdrop_net_base::Addr;

/// 保序并集：`first` 全部在前，`second` 中未出现过的接在后面，整体去重。
///
/// 地址集合的合并在内核里出现了三处（可拨地址 = 监听 ∪ 外部、外部地址视图 =
/// 声明 ∪ 自动确认、单份声明自身的去重），三处要的都是这一个语义。
///
/// **为什么保序而不是用 `HashSet`。** 这些结果会被拿来做相等比较，以判断状态视图是否
/// 真的变了（变了才广播一轮给订阅者与 lookup）。`HashSet` 的迭代顺序不保证稳定，
/// 同样的内容会算出不同顺序的 `Vec`，于是每次重算都被判为「变了」——白广播，而且
/// 那正是最难查的一类问题：功能全对，只是话变多了。
///
/// 集合规模是个位数，线性 `contains` 比建哈希表更快也更简单。
pub(crate) fn union_preserving_order(first: &[Addr], second: &[Addr]) -> Vec<Addr> {
    let mut out: Vec<Addr> = Vec::with_capacity(first.len() + second.len());
    for addr in first.iter().chain(second) {
        if !out.contains(addr) {
            out.push(addr.clone());
        }
    }
    out
}

/// 保序去重（[`union_preserving_order`] 与空集的并）。
///
/// 单独给个名字是因为调用点读起来完全是另一件事 —— `union(&x, &[])` 会让读者去反推
/// 那个空切片的用意。
pub(crate) fn dedup_preserving_order(addrs: &[Addr]) -> Vec<Addr> {
    union_preserving_order(addrs, &[])
}

/// 把一批监听地址映射成「本机在 `ip` 这个公网 IP 上的可达形态」。
///
/// 静态 1:1 NAT / 直接持有公网 IP 的节点用它从监听集合派生公网地址
/// （见 [`Builder::external_ip`](crate::endpoint::builder::Builder::external_ip)）。
///
/// 两条判据：
///
/// - **circuit 地址排除。** `/…/p2p/<relay>/p2p-circuit` 里的 IP 段是**中继方**的，
///   换成本机公网 IP 得到的是一条既拨不通、又指认错了中继的地址；而它会随 identify
///   广播给每个对端，对端照着拨只会连到一台无关的机器。
/// - **改写后去重。** 监听 `0.0.0.0` 会展开成每张网卡一条，改写后全部重合。不去重的话
///   每轮算出的集合个数随网卡数变化而内容相同，调用方的「变了没有」判定恒为真。
pub(crate) fn map_to_public_ip(listen: &[Addr], ip: std::net::IpAddr) -> Vec<Addr> {
    let mapped: Vec<Addr> = listen
        .iter()
        .filter(|addr| !addr.is_circuit())
        .map(|addr| addr.with_ip(ip))
        .collect();
    dedup_preserving_order(&mapped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Addr {
        s.parse().unwrap()
    }

    #[test]
    fn keeps_first_then_new_ones_from_second() {
        let first = [a("/ip4/1.1.1.1/tcp/1"), a("/ip4/2.2.2.2/tcp/2")];
        let second = [a("/ip4/2.2.2.2/tcp/2"), a("/ip4/3.3.3.3/tcp/3")];

        assert_eq!(
            union_preserving_order(&first, &second),
            vec![
                a("/ip4/1.1.1.1/tcp/1"),
                a("/ip4/2.2.2.2/tcp/2"),
                a("/ip4/3.3.3.3/tcp/3"),
            ]
        );
    }

    /// `first` 自身带重复也要去掉——宿主整份声明时不该被要求先自己去重。
    #[test]
    fn dedups_within_first() {
        let first = [a("/ip4/1.1.1.1/tcp/1"), a("/ip4/1.1.1.1/tcp/1")];

        assert_eq!(
            union_preserving_order(&first, &[]),
            vec![a("/ip4/1.1.1.1/tcp/1")]
        );
    }

    /// 监听地址的 IP 段换成公网 IP，端口与其余段（含 certhash）原样保留。
    #[test]
    fn maps_listen_addresses_onto_the_public_ip() {
        let listen = [
            a("/ip4/192.168.1.5/tcp/4001"),
            a(
                "/ip4/192.168.1.5/udp/4004/quic-v1/webtransport/certhash/uEiDDq4_xNyDorZBH3TlGazyJdOWSwvo4PUo5YHFMrvDE8g",
            ),
        ];

        assert_eq!(
            map_to_public_ip(&listen, "203.0.113.10".parse().unwrap()),
            vec![
                a("/ip4/203.0.113.10/tcp/4001"),
                a(
                    "/ip4/203.0.113.10/udp/4004/quic-v1/webtransport/certhash/uEiDDq4_xNyDorZBH3TlGazyJdOWSwvo4PUo5YHFMrvDE8g"
                ),
            ]
        );
    }

    /// **护栏：circuit 地址不能被改写。** 那条地址里的 IP 是**中继方**的——换成本机公网
    /// IP 会得到一条既拨不通、又指认错了中继的地址，并随 identify 广播给每个对端。
    #[test]
    fn leaves_circuit_addresses_out() {
        let listen = [
            a(
                "/ip4/198.51.100.7/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN/p2p-circuit",
            ),
            a("/ip4/192.168.1.5/tcp/4001"),
        ];

        assert_eq!(
            map_to_public_ip(&listen, "203.0.113.10".parse().unwrap()),
            vec![a("/ip4/203.0.113.10/tcp/4001")],
            "circuit 地址必须整条排除，而不是把中继的 IP 改成本机公网 IP"
        );
    }

    /// 监听 `0.0.0.0` 会展开成每张网卡一条，改写后全部重合——必须去重，否则调用方的
    /// 「变了没有」判定会随网卡数恒为真。
    #[test]
    fn dedups_addresses_that_collide_after_rewriting() {
        let listen = [
            a("/ip4/127.0.0.1/tcp/4001"),
            a("/ip4/192.168.1.5/tcp/4001"),
            a("/ip4/10.0.0.3/tcp/4001"),
        ];

        assert_eq!(
            map_to_public_ip(&listen, "203.0.113.10".parse().unwrap()),
            vec![a("/ip4/203.0.113.10/tcp/4001")]
        );
    }

    /// 同样的内容必须算出**同样顺序**的结果，否则调用方的「变了没有」比较会恒为真。
    /// 这条是本函数存在的理由，不是锦上添花。
    #[test]
    fn is_deterministic_for_equal_inputs() {
        let first = [a("/ip4/1.1.1.1/tcp/1"), a("/ip4/2.2.2.2/tcp/2")];
        let second = [a("/ip4/3.3.3.3/tcp/3")];

        assert_eq!(
            union_preserving_order(&first, &second),
            union_preserving_order(&first, &second)
        );
    }
}
