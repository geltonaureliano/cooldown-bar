const API = "https://api.github.com";
const UPLOADS = "https://uploads.github.com";

export class ApiError extends Error {
  constructor(status, method, path) {
    // Never include request headers, secrets, or arbitrary response bodies in logs.
    super(`GitHub API: ${method} ${path} retornou HTTP ${status}.`);
    this.status = status;
  }
}

export class GitHub {
  constructor(repository, token, { fetchImpl = fetch, sleep = (ms) => new Promise((r) => setTimeout(r, ms)) } = {}) {
    if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository ?? "")) throw new Error("GITHUB_REPOSITORY inválido.");
    if (!token) throw new Error("GITHUB_TOKEN ausente.");
    this.repository = repository;
    this.base = `/repos/${repository}`;
    this.fetch = fetchImpl;
    this.sleep = sleep;
    this.token = token;
  }

  async request(method, path, body, { allow404 = false, binary = false } = {}) {
    const readOnly = method === "GET" || path.endsWith("/generate-notes");
    for (let attempt = 0; ; attempt++) {
      let response;
      try {
        response = await this.fetch(`${binary ? UPLOADS : API}${this.base}${path}`, {
          method,
          headers: {
            Accept: "application/vnd.github+json",
            Authorization: `Bearer ${this.token}`,
            "X-GitHub-Api-Version": "2026-03-10",
            "Content-Type": binary ? "application/octet-stream" : "application/json",
          },
          body: body === undefined ? undefined : binary ? body : JSON.stringify(body),
          signal: AbortSignal.timeout(binary ? 180_000 : 30_000),
          redirect: "error",
        });
      } catch {
        // A write may have succeeded before a connection failed. Do not repeat it
        // blindly: a workflow rerun reconciles existing tags/drafts/assets first.
        if (!readOnly || attempt >= 2) throw new Error(`Falha de conexão com o GitHub (${method} ${path}). Execute novamente para reconciliar o estado.`);
        await this.sleep(1000 * 2 ** attempt);
        continue;
      }
      if (response.status === 404 && allow404) return null;
      if (response.ok) return response.status === 204 ? null : response.json();
      if (readOnly && attempt < 2 && (response.status >= 500 || response.status === 429)) {
        const retry = Number(response.headers.get("retry-after"));
        await this.sleep(Math.min(30_000, Math.max(1000 * 2 ** attempt, Number.isFinite(retry) ? retry * 1000 : 0)));
        continue;
      }
      throw new ApiError(response.status, method, path);
    }
  }

  async tagCommit(tag) {
    const ref = await this.request("GET", `/git/ref/tags/${encodeURIComponent(tag)}`, undefined, { allow404: true });
    if (!ref) return null;
    let object = ref.object;
    for (let depth = 0; depth < 8; depth++) {
      if (object.type === "commit") return object.sha;
      if (object.type !== "tag") break;
      object = (await this.request("GET", `/git/tags/${object.sha}`)).object;
    }
    throw new Error(`A tag ${tag} não aponta para um commit válido.`);
  }

  async release(tag) {
    const published = await this.request("GET", `/releases/tags/${encodeURIComponent(tag)}`, undefined, { allow404: true });
    if (published) return published;
    // The tag endpoint only promises published releases. Search the authenticated
    // release list as well, otherwise retries can fail to find their own draft.
    for (let page = 1; page <= 100; page++) {
      const batch = await this.request("GET", `/releases?per_page=100&page=${page}`);
      const existing = batch.find((release) => release.tag_name === tag);
      if (existing) return existing;
      if (batch.length < 100) return null;
    }
    throw new Error("Limite de paginação atingido ao procurar a release; revisão manual necessária.");
  }

  async assets(id) {
    const assets = [];
    for (let page = 1; page <= 100; page++) {
      const batch = await this.request("GET", `/releases/${id}/assets?per_page=100&page=${page}`);
      assets.push(...batch);
      if (batch.length < 100) return assets;
    }
    throw new Error("Quantidade inesperada de arquivos na release.");
  }
}
