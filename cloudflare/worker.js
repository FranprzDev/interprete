// Sirve la web y hace de espejo de Hugging Face en /hf/ (mismo origen, sin problemas de CORS).
const ALLOWED = /^\/(onnx-community|Xenova)\/[\w.\-]+\/resolve\//;

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname.startsWith("/hf/")) {
      const path = url.pathname.slice(3);
      if (!ALLOWED.test(path)) return new Response("no permitido", { status: 403 });
      const upstream = await fetch("https://huggingface.co" + path + url.search, {
        method: request.method === "HEAD" ? "HEAD" : "GET",
        headers: request.headers.has("range") ? { range: request.headers.get("range") } : {},
        redirect: "follow",
        cf: { cacheEverything: true, cacheTtl: 86400 },
      });
      const h = new Headers(upstream.headers);
      h.set("cache-control", "public, max-age=86400");
      h.delete("set-cookie");
      return new Response(upstream.body, { status: upstream.status, headers: h });
    }
    return env.ASSETS.fetch(request);
  },
};
