# SeaORM 2.0 Nested ActiveModel

## 目录

- [概览](#概览)
- [ActiveModel Builder](#activemodel-builder)
- [1-1 关系操作](#1-1-关系操作)
- [1-N 关系操作](#1-n-关系操作)
- [M-N 关系操作](#m-n-关系操作)
- [变更检测](#变更检测)
- [级联删除](#级联删除)
- [幂等性](#幂等性)

## 概览

Nested ActiveModel 让你可以一次操作保存嵌套对象树。SeaORM 自动：
1. 遍历对象树检测变更
2. 解析外键依赖
3. 按正确顺序执行 INSERT/UPDATE
4. 在事务中执行

仅适用于 `#[sea_orm::model]` 定义的 Entity。

## ActiveModel Builder

```rust
let user = user::ActiveModel::builder()
    .set_name("Bob")
    .set_email("bob@sea-ql.org")
    .set_profile(
        profile::ActiveModel::builder().set_picture("image.jpg")
    )
    .add_post(
        post::ActiveModel::builder()
            .set_title("Nice weather")
            .add_tag(tag::ActiveModel::builder().set_tag("sunny")),
    )
    .save(db).await?;
```

### save / insert / update 差异

| 方法 | 行为 | 何时用 |
|------|------|--------|
| `save(db)` | 自动按主键状态选 INSERT 或 UPDATE（推荐默认） | 大部分场景 |
| `insert(db)` | 强制 INSERT，主键 `Set` 也会冲突 | 明确创建新记录 |
| `update(db)` | 强制 UPDATE，主键必须 `Set` | 明确更新 |
| `delete(db)` | 客户端级联删除（详见下方） | 删除带子记录的实体 |

### 取出嵌套 Model 并修改

```rust
// 1. 用 .take() 取出 HasOne，转为 ActiveModel 修改
bob.profile
    .take()              // Option<Model> → Option，原字段置空
    .unwrap()
    .into_active_model()
    .set_picture("Landscape")
    .save(db).await?;

// 2. 直接 in-place 改 HasOne 字段
let mut bob = bob.into_active_model();
bob.profile.as_mut().unwrap().picture = Set("Hiking.jpg".into());
bob.save(db).await?;
```

### HasMany 字段操作

```rust
let mut bob = bob.into_active_model();

// push — 追加（默认 append 语义）
bob.posts.push(post::ActiveModel::builder().set_title("Post A"));

// 链式 push
bob.posts
    .push(post::ActiveModel::builder().set_title("Post A"))
    .push(post::ActiveModel::builder().set_title("Post B"));

// take + as_mut_vec — 取出 Vec 进行任意操作
let mut tags = post.tags.take();
tags.as_mut_vec().remove(0);    // 移除第 0 项
post.tags.replace_all(tags);    // 切换为 replace 语义

// replace_all — 指定精确集合（删除不在列表中的）
bob.posts.replace_all([post_1, post_2]);
```

等价于手动执行：

```rust
let txn = db.begin().await?;
let user = user::ActiveModel { name: Set("Bob".into()), .. }.insert(&txn).await?;
let profile = profile::ActiveModel { user_id: Set(user.id), .. }.insert(&txn).await?;
let post = post::ActiveModel { user_id: Set(user.id), .. }.insert(&txn).await?;
let tag = tag::ActiveModel { tag: Set("sunny".into()), .. }.insert(&txn).await?;
post_tag::ActiveModel { post_id: Set(post.id), tag_id: Set(tag.id) }.insert(&txn).await?;
txn.commit().await?;
```

## 1-1 关系操作

**不论嵌套方向如何，SeaORM 都会按正确顺序执行。**

```rust
// user 包含 profile
user::ActiveModel::builder()
    .set_name("Bob")
    .set_profile(profile::ActiveModel::builder().set_picture("img.jpg"))
    .save(db).await?;

// 反向：profile 包含 user（同样有效）
profile::ActiveModel::builder()
    .set_user(user::ActiveModel::builder().set_name("Alice"))
    .set_picture("img.jpg")
    .save(db).await?;
```

### 更新嵌套 1-1

```rust
let mut bob = bob.into_active_model();
bob.profile.as_mut().unwrap().picture = Set("Hiking.jpg");
bob.save(db).await?;
// SQL: UPDATE profile SET picture = 'Hiking.jpg' WHERE id = ?
```

## 1-N 关系操作

### Append（默认，非破坏性）

```rust
let mut bob = bob.into_active_model();
bob.posts.push(post::ActiveModel::builder().set_title("Another weekend"));
bob.save(db).await?;
// SQL: INSERT INTO post (user_id, title) VALUES (bob.id, 'Another weekend')
```

### Replace（指定精确集合）

```rust
bob.posts.replace_all([]);        // 删除所有 post
bob.posts.replace_all([post_1]);  // 只保留 post_1，删除其余
// SQL: SELECT FROM post WHERE user_id = bob.id
//      DELETE FROM post WHERE id = 2  (不在列表中的)
```

## M-N 关系操作

M-N 关系不是"属于"关系，是**关联**。删除/替换不会删除关联实体，只操作 junction 表。

### 添加关联

```rust
let sunny = tag::ActiveModel::builder().set_tag("sunny").save(db).await?;

let post = post::ActiveModel::builder()
    .set_title("A sunny day")
    .set_user(bob)
    .add_tag(sunny)                                           // 已有 tag
    .add_tag(tag::ActiveModel::builder().set_tag("outdoor"))  // 新 tag
    .save(db).await?;
// SQL:
// INSERT INTO post ..
// INSERT INTO tag (tag) VALUES ('outdoor')
// INSERT INTO post_tag (post_id, tag_id) VALUES (post.id, sunny.id), (post.id, outdoor.id)
```

### 替换关联

```rust
post.tags.replace_all([outdoor]);  // 解除 sunny 关联
// SQL: DELETE FROM post_tag WHERE (post_id, tag_id) IN ((post.id, sunny.id))
// 注意：tag "sunny" 本身不会被删除
```

## 变更检测

每个 `ActiveValue` 是三态：

```rust
pub enum ActiveValue<V> {
    Set(V),        // 已修改
    Unchanged(V),  // 从数据库查出，未修改
    NotSet,        // 未设置
}
```

嵌套 ActiveModel 也参与变更检测：

```rust
let mut bob: user::ActiveModel = ..;
bob.posts[0].title = Set("New title".into());           // 标记为 Set
bob.posts[0].comments[0].comment = Set("updated".into());
bob.posts[1].comments.push(
    comment::ActiveModel::builder().set_comment("new!")
);
bob.save(db).await?;
// SQL:
// UPDATE post SET title = '..' WHERE id = ?
// UPDATE comment SET comment = '..' WHERE id = ?
// INSERT INTO comment (post_id, comment) VALUES (?, '..')
```

只有 `Set` 状态的字段才会被更新，避免过度写入和竞态条件。

## 级联删除

SeaORM 支持客户端级联删除（无需数据库 ON DELETE CASCADE）：

```rust
let user = user::Entity::find_by_id(4).one(db).await?.unwrap();
user.cascade_delete(db).await?;
// 或等价写法
user.into_ex().delete(db).await?;
```

执行顺序（自底向上）：

```sql
-- 查询并删除 profile
SELECT FROM profile WHERE user_id = 4
DELETE FROM profile WHERE id = ?
-- 查询 post 及其子依赖
SELECT FROM post WHERE user_id = 4
SELECT FROM comment WHERE post_id IN (..)
DELETE FROM comment WHERE id IN (..)
SELECT FROM post_tag WHERE post_id IN (..)
DELETE FROM post_tag WHERE (post_id, tag_id) IN (..)
-- 删除 post
DELETE FROM post WHERE id IN (..)
-- 最后删除 user
DELETE FROM user WHERE id = 4
```

### Weak BelongsTo（弱关联）

外键可空时，级联删除不会删除子记录，而是将外键设为 NULL：

```rust
// attachment.rs
pub post_id: Option<i32>,  // nullable
```

删除 post 时，attachment 的 `post_id` 会被设为 `NULL`，attachment 本身保留。

## 幂等性

保存同一 ActiveModel 两次是幂等的（第二次是 no-op）：

```rust
let post = post.save(db).await?;
let post = post.save(db).await?;  // no-op，所有字段都是 Unchanged
```

**建议**：使用 `save()` 作为默认操作（自动判断 insert 还是 update），只在明确需要"新建"时用 `insert()`。
