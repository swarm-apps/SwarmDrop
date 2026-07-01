# meaningful-transfer-test-coverage Specification

## Purpose
TBD - created by archiving change frontend-and-test-followups. Update Purpose after archive.
## Requirements
### Requirement: 自动化用例必须真正执行行为断言或显式跳过

信任策略、收件箱、LAN helper 等以功能命名的自动化用例 SHALL 在 CI 默认环境下真正执行其核心行为断言;当用例依赖 CI 默认环境无法满足的前置条件(如已配对设备数据态、可绑定私有 IPv4)时,该用例 MUST 显式跳过(标注为条件覆盖 / `#[ignore]`),而非在前置不满足时静默走空仍报通过。

#### Scenario: 信任策略 maestro 用例在有数据态时真正断言

- **WHEN** 运行 device-policy smoke 且已提供"预置一台已配对设备"的夹具
- **THEN** `device-card-0` 数据态可达,信任策略相关断言被真正执行(而非被 `when visible` 网关跳过)

#### Scenario: 无法满足前置的用例显式跳过而非假通过

- **WHEN** `e2e_lan_helper` 运行环境无可绑定的私有 IPv4 地址
- **THEN** 该用例显式跳过(`#[ignore]` 或标记跳过),测试报告中不计为"已通过的有效覆盖"

#### Scenario: 永真断言被消除

- **WHEN** 审查 maestro smoke 用例
- **THEN** 不存在 `when: visible <X>` 紧接 `assertVisible <同一 X>` 这类恒真断言;断言针对动作产生的新状态

