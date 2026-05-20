# Legal Information

This document explains the legal posture and operating boundaries of
`hearthstone-linux-gui`. It is intended to make clear that this project is an
independent compatibility launcher and that the maintainers do not claim,
copy, redistribute, or replace Blizzard Entertainment's rights in Hearthstone,
Battle.net, or related Blizzard properties.

This document is not legal advice. If you need legal certainty for your own
use, distribution, or modification of this project, consult a qualified lawyer
in your jurisdiction.

## Ownership Of Blizzard Properties

Hearthstone, Blizzard Entertainment, Battle.net, Blizzard Battle.net, and any
related names, logos, game clients, artwork, audio, video, text, story
elements, characters, card designs, user interfaces, services, and other
Hearthstone or Blizzard materials are owned by Blizzard Entertainment, Inc. or
its affiliates and licensors.

All Blizzard trademarks and registered trademarks remain the property of
Blizzard Entertainment, Inc. or its affiliates. Any use of Blizzard names in
this repository is nominative: it identifies the game that this independent
launcher is intended to install and launch for users who choose to use it. This
project does not claim ownership of, exclusive rights in, endorsement by, or a
license from Blizzard Entertainment.

Relevant official Blizzard legal resources include:

- Blizzard Legal Documentation: https://www.blizzard.com/legal
- Blizzard End User License Agreement: https://www.blizzard.com/legal/08b946df-660a-40e4-a072-1fbde65173b1/blizzard-end-user-license-agreement
- Blizzard Logo and Trademark Guidelines: https://www.blizzard.com/legal/8bcb0794-6641-4ce3-a573-8eb243bab342/blizzard-entertainment-logo-and-trademark-guidelines
- Blizzard Online Privacy Policy: https://www.blizzard.com/en-us/legal/a4380ee5-5c8d-4e3b-83b7-ea26d01a9918/

## No Affiliation Or Endorsement

`hearthstone-linux-gui` is an unofficial community project. It is not produced,
published, sponsored, approved, endorsed, maintained, or supported by Blizzard
Entertainment, Battle.net, Microsoft, Activision Blizzard, or any of their
affiliates.

The project name and documentation use the word "Hearthstone" only to describe
compatibility with Blizzard's game. The project should not be presented as an
official Hearthstone product, an official Blizzard launcher, an official Linux
port, or a Blizzard-approved distribution channel.

## What This Repository Distributes

This repository distributes only the source code, build scripts, packaging
metadata, compatibility code, desktop metadata, and documentation needed for
the open-source launcher itself.

The launcher code in this repository is licensed under the MIT License unless a
file states otherwise. The MIT License applies only to this project's original
code and documentation. It does not grant any license to Blizzard software,
Hearthstone game content, Blizzard services, Blizzard trademarks, Battle.net
accounts, or any other third-party property.

## What This Repository Does Not Distribute

This repository and its release packages are not intended to include or
redistribute proprietary Hearthstone or Blizzard content, including but not
limited to:

- Hearthstone game client files.
- Hearthstone card art, music, sound effects, cinematics, fonts, text,
  localization data, or other game assets.
- Blizzard or Hearthstone logos, icons, branding packages, or marketing
  materials.
- Battle.net client binaries or proprietary Blizzard service components.
- Blizzard account credentials, session tokens, entitlement data, or private
  user information.

During installation, the launcher downloads official game data from Blizzard's
upstream distribution infrastructure for the selected region and locale. The
project does not host those proprietary files, mirror them, sell them, or
publish them as part of this repository.

## Compatibility Boundaries

The purpose of this project is limited to Linux compatibility: installing the
official Hearthstone game data selected by the user, preparing a Linux runtime
layout, handling the user's Battle.net login callback, and launching the game
locally.

The project is not intended to:

- Bypass Blizzard account login, authentication, entitlement checks, region
  restrictions, payment requirements, or server-side authorization.
- Circumvent anti-cheat, DRM, encryption, access controls, or technical
  protection measures.
- Modify gameplay, automate gameplay, provide bots, cheats, hacks, memory
  patching, packet manipulation, or competitive advantage tools.
- Interfere with Blizzard servers, Battle.net services, matchmaking, commerce,
  account systems, telemetry, or security mechanisms.
- Enable piracy, unauthorized copies, private servers, asset extraction, or
  redistribution of Blizzard content.
- Mislead users into believing the project is official, endorsed, supported,
  or operated by Blizzard.

If any code, documentation, packaging, or release artifact appears to cross one
of these boundaries, that is contrary to the intended scope of this project and
should be reported so it can be reviewed and corrected.

## User Responsibilities

Users are responsible for complying with Blizzard's applicable agreements,
policies, regional rules, and laws when using Hearthstone, Battle.net, and this
launcher.

Users should only use this launcher with a Blizzard account and game access
they are authorized to use. Installing or launching Hearthstone through this
project does not change Blizzard's terms, grant additional rights, remove
license restrictions, or create any relationship between the user and this
project's maintainers beyond use of the open-source launcher.

Blizzard may change its game client, services, login flow, distribution
systems, policies, or technical requirements at any time. This project does not
guarantee continued compatibility, access, availability, or compliance for any
particular user or jurisdiction.

## Privacy And Credentials

The project should avoid collecting private Blizzard account credentials. Login
is performed through the user's browser and Blizzard/Battle.net flow. The
launcher may receive and store local authentication data needed to start the
game, and that data is intended to remain on the user's own machine under the
project's managed data directory.

Maintainers should not ask users to publish Blizzard credentials, session
tokens, personal account data, or private logs containing sensitive tokens in
issues, discussions, bug reports, or support requests.

## Takedown And Rights Holder Requests

The maintainers intend to respect Blizzard's intellectual-property rights,
trademarks, contractual rights, service rules, and applicable law. If Blizzard
Entertainment, its affiliates, or another rights holder believes that material
in this repository infringes or otherwise violates their rights, they are
encouraged to open an issue or contact the repository owner through GitHub so
the concern can be reviewed promptly.

The maintainers are willing to remove, rename, reword, or modify project
materials where appropriate to avoid confusion, unauthorized use, or legal
conflict.

## Third-Party Components

This project may use open-source dependencies and compatibility stubs. Those
components remain subject to their own licenses and notices. Stub libraries in
this repository are intended only to satisfy runtime compatibility needs; they
are not Blizzard SDKs, do not contain Blizzard proprietary implementation code,
and do not grant access to Blizzard services beyond what the official game
client and user account already permit.

## 中文法律说明

本文档用于说明 `hearthstone-linux-gui` 的法律立场和项目边界。本项目是独立的 Linux
兼容启动器，维护者无意主张、复制、分发、替代或侵犯 Blizzard Entertainment 对
Hearthstone / 炉石传说、Battle.net 以及相关 Blizzard 资产享有的任何权利。

本文档不是法律意见。如果你需要判断自己使用、分发或修改本项目在特定地区是否具有法律确定性，请咨询具备相应资质的律师。

## Blizzard 资产的权属

Hearthstone、炉石传说、Blizzard Entertainment、Battle.net、Blizzard
Battle.net 以及相关名称、标志、游戏客户端、美术、音频、视频、文本、剧情元素、角色、卡牌设计、用户界面、服务和其他炉石传说或
Blizzard 资料，均归 Blizzard Entertainment, Inc. 或其关联方、许可方所有。

Blizzard 的所有商标和注册商标仍归 Blizzard Entertainment, Inc. 或其关联方所有。本仓库中对
Blizzard、Hearthstone、炉石传说或 Battle.net 名称的使用，仅用于说明本非官方启动器兼容的目标游戏，不表示本项目拥有这些名称、获得
Blizzard 授权、得到 Blizzard 认可，或与 Blizzard 存在隶属、合作、赞助、发布、维护、支持关系。

可参考的 Blizzard 官方法律资源包括：

- Blizzard 法律文档：https://www.blizzard.com/legal
- Blizzard 最终用户许可协议：https://www.blizzard.com/legal/08b946df-660a-40e4-a072-1fbde65173b1/blizzard-end-user-license-agreement
- Blizzard 标志和商标指南：https://www.blizzard.com/legal/8bcb0794-6641-4ce3-a573-8eb243bab342/blizzard-entertainment-logo-and-trademark-guidelines
- Blizzard 在线隐私政策：https://www.blizzard.com/en-us/legal/a4380ee5-5c8d-4e3b-83b7-ea26d01a9918/

## 非官方且未获背书

`hearthstone-linux-gui` 是非官方社区项目，不由 Blizzard Entertainment、Battle.net、Microsoft、Activision
Blizzard 或其任何关联方制作、发布、赞助、批准、认可、维护或支持。

项目名称和文档中出现 "Hearthstone" 或“炉石传说”，仅用于说明本项目面向该游戏提供 Linux
兼容启动能力。本项目不应被表述为官方炉石传说产品、官方 Blizzard 启动器、官方 Linux 移植版或 Blizzard 授权的分发渠道。

## 本仓库分发的内容

本仓库只分发开源启动器自身所需的源代码、构建脚本、打包元数据、兼容代码、桌面元数据和文档。

除非具体文件另有说明，本仓库中的启动器代码采用 MIT License 授权。MIT License
仅适用于本项目原创代码和文档，不授权任何 Blizzard 软件、炉石传说游戏内容、Blizzard 服务、Blizzard 商标、Battle.net
账号或其他第三方财产。

## 本仓库不分发的内容

本仓库及其发布包无意包含或再分发任何炉石传说或 Blizzard 专有内容，包括但不限于：

- 炉石传说游戏客户端文件。
- 炉石传说卡牌美术、音乐、音效、动画、字体、文本、本地化数据或其他游戏资产。
- Blizzard 或炉石传说标志、图标、品牌包或市场宣传材料。
- Battle.net 客户端二进制文件或 Blizzard 专有服务组件。
- Blizzard 账号凭据、会话 token、权益数据或用户私人信息。

安装过程中，启动器会根据用户选择的区服和语言，从 Blizzard 上游官方分发基础设施下载官方游戏数据。本项目不托管、不镜像、不销售，也不把这些专有文件作为仓库或发布包的一部分发布。

## 兼容性边界

本项目的目的仅限于 Linux 兼容：安装用户选择的官方炉石传说游戏数据，准备 Linux 运行时布局，处理用户的 Battle.net
登录回调，并在本地启动游戏。

本项目无意实施以下行为：

- 绕过 Blizzard 账号登录、身份验证、权益检查、区服限制、付费要求或服务端授权。
- 规避反作弊、DRM、加密、访问控制或技术保护措施。
- 修改游戏玩法，提供自动化游戏、机器人、作弊、外挂、内存补丁、封包篡改或任何竞争优势工具。
- 干扰 Blizzard 服务器、Battle.net 服务、匹配系统、商城系统、账号系统、遥测或安全机制。
- 促进盗版、未经授权的副本、私服、资产提取或 Blizzard 内容再分发。
- 误导用户相信本项目是 Blizzard 官方项目，或由 Blizzard 认可、支持、运营。

如果任何代码、文档、打包内容或发布产物看起来越过上述边界，这都不符合本项目的预期范围，应及时报告以便审查和修正。

## 用户责任

用户在使用炉石传说、Battle.net 和本启动器时，应自行遵守 Blizzard 适用的协议、政策、地区规则和法律。

用户应只在自己有权使用的 Blizzard 账号和游戏访问权限下使用本启动器。通过本项目安装或启动炉石传说，不会改变 Blizzard
条款，不会授予额外权利，不会移除许可限制，也不会在用户和本项目维护者之间创建超出开源启动器使用范围之外的关系。

Blizzard 可能随时更改其游戏客户端、服务、登录流程、分发系统、政策或技术要求。本项目不保证对任何特定用户、地区或司法辖区持续兼容、可访问、可用或合规。

## 隐私和凭据

本项目应避免收集用户的 Blizzard 账号密码。登录流程通过用户浏览器和 Blizzard/Battle.net 流程完成。启动器可能会接收并在本地保存启动游戏所需的认证数据，这些数据预期只保存在用户自己机器上的项目数据目录中。

维护者不应要求用户在 issue、discussion、bug report 或支持请求中公开 Blizzard 凭据、会话 token、账号私人数据，或包含敏感 token 的私密日志。

## 权利人请求

维护者意图尊重 Blizzard 的知识产权、商标权、合同权利、服务规则和适用法律。如果 Blizzard Entertainment、其关联方或其他权利人认为本仓库中的材料侵犯或违反其权利，欢迎通过
GitHub issue 或仓库所有者联系方式提出，以便尽快审查。

维护者愿意在适当情况下删除、重命名、改写或修改项目材料，以避免混淆、未经授权使用或法律冲突。

## 第三方组件

本项目可能使用开源依赖和兼容性 stub。这些组件仍受其各自许可证和声明约束。本仓库中的 stub
库仅用于满足运行时兼容需求；它们不是 Blizzard SDK，不包含 Blizzard 专有实现代码，也不授予超出官方游戏客户端和用户账号本身允许范围之外的
Blizzard 服务访问能力。
