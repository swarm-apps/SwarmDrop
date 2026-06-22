import { createTokenizer } from "@orama/tokenizers/mandarin";
import { createFromSource } from "fumadocs-core/search/server";
import { source } from "@/lib/source";

export const revalidate = false;

// 文档主体是中文：用 mandarin 分词器建静态索引，中文词可命中，英文术语（API/命令）
// 走空白切分照样可搜。客户端 initOrama 必须用同一分词器，否则索引/查询不对齐。
export const { staticGET: GET } = createFromSource(source, {
  tokenizer: createTokenizer(),
});
