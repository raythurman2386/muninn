// Build-time helpers for the /download/ page. The Astro data loader in
// releases-loader.ts calls loadLatestRelease(), which talks to the GitHub
// Releases API and falls back to the committed snapshot on any failure.

import snapshotJson from '../data/github-release-snapshot.json';

export const RELEASE_REPO = 'raythurman2386/muninn';
export const COSIGN_OIDC_ISSUER = 'https://token.actions.githubusercontent.com';
export const COSIGN_IDENTITY_REGEXP =
  '^https://github.com/raythurman2386/muninn/\\.github/workflows/release\\.yml@refs/tags/';
export const SOURCE_URL = 'https://github.com/raythurman2386/muninn#quick-start';

const DEFAULT_API = `https://api.github.com/repos/${RELEASE_REPO}/releases/latest`;
const FETCH_TIMEOUT_MS = 8_000;

export type ReleaseSource = 'api' | 'snapshot';
export type AssetKind = 'archive' | 'deb' | 'checksums' | 'sigstore' | 'other';
export type OsFamily = 'linux' | 'macos' | 'windows' | 'other';

export interface SnapshotAsset {
  name: string;
  size: number;
  url: string;
  contentType?: string;
  digest?: string;
  sha256?: string | null;
}

export interface ReleaseSnapshot {
  tag: string;
  name: string;
  htmlUrl: string;
  publishedAt: string;
  body: string;
  assets: SnapshotAsset[];
  checksumsTxt: string;
}

export interface ReleaseAsset {
  name: string;
  size: number;
  url: string;
  sha256: string | null;
  kind: AssetKind;
  cardId: string | null;
}

export interface OsCard {
  id: string;
  os: OsFamily;
  arch: string;
  title: string;
  archLabel: string;
  notes: string[];
  assets: ReleaseAsset[];
}

export interface LatestRelease {
  source: ReleaseSource;
  tag: string;
  name: string;
  htmlUrl: string;
  publishedAt: string;
  body: string;
  assets: ReleaseAsset[];
  checksumsUrl: string | null;
  sigstoreUrl: string | null;
  checksumsTxt: string;
  cards: OsCard[];
  cosignOidcIssuer: string;
  cosignCertificateIdentity: string;
  cosignCertificateIdentityRegexp: string;
}

export const snapshot = snapshotJson as ReleaseSnapshot;

export interface LoaderLog {
  info: (message: string) => void;
  warn: (message: string) => void;
}

const silent: LoaderLog = { info() {}, warn() {} };

export function parseChecksums(text: string): Map<string, string> {
  const out = new Map<string, string>();
  for (const line of text.split(/\r?\n/)) {
    const match = /^([a-fA-F0-9]{64})\s+\*?(.+?)\s*$/.exec(line);
    if (match) out.set(match[2], match[1].toLowerCase());
  }
  return out;
}

function shaFromDigest(digest: string | undefined, fallback?: string | null): string | null {
  if (digest && digest.startsWith('sha256:')) return digest.slice('sha256:'.length).toLowerCase();
  return fallback ?? null;
}

export function classifyAsset(name: string): { kind: AssetKind; cardId: string | null } {
  const lower = name.toLowerCase();
  if (lower === 'checksums.txt') return { kind: 'checksums', cardId: null };
  if (lower.endsWith('.sigstore.json') || lower.includes('sigstore')) return { kind: 'sigstore', cardId: null };

  const deb = lower.endsWith('.deb');
  const archive = lower.endsWith('.tar.gz') || lower.endsWith('.zip') || lower.endsWith('.tgz');

  let os: OsFamily | null = null;
  let arch: string | null = null;

  if (/(^|[_-])(amd64)([_-]|\.|$)/.test(lower) || /x86_64/.test(lower)) arch = 'x86_64';
  else if (/(^|[_-])(arm64|aarch64)([_-]|\.|$)/.test(lower)) arch = 'aarch64';

  if (deb) os = 'linux';
  else if (/_linux_/.test(lower) || lower.includes('linux')) os = 'linux';
  else if (/_macos_/.test(lower) || /_darwin_/.test(lower) || lower.includes('macos')) os = 'macos';
  else if (/_windows_/.test(lower) || /_win(64|32)?_/.test(lower) || lower.includes('windows')) os = 'windows';

  if (!os || !arch) {
    const kind: AssetKind = deb ? 'deb' : archive ? 'archive' : 'other';
    const cardId = os || arch ? `${os ?? 'other'}-${arch ?? 'unknown'}` : null;
    return { kind, cardId };
  }

  return { kind: deb ? 'deb' : archive ? 'archive' : 'other', cardId: `${os}-${arch}` };
}

const CARD_META: Record<string, Pick<OsCard, 'os' | 'arch' | 'title' | 'archLabel' | 'notes'>> = {
  // glibc floors measured on the v0.8.2 release binaries (readelf -V / dyn-syms):
  // x86_64 links GLIBC_2.43 (asinf, atan2f, acoshf, coshf, sinhf, acosf);
  // aarch64's highest needed version is GLIBC_2.39 (pidfd_spawnp, pidfd_getpid).
  'linux-x86_64': {
    os: 'linux',
    arch: 'x86_64',
    title: 'Linux',
    archLabel: 'x86_64',
    notes: ['Needs glibc 2.43 or newer. Ubuntu 24.04 and Debian 13 are too old.'],
  },
  'linux-aarch64': {
    os: 'linux',
    arch: 'aarch64',
    title: 'Linux',
    archLabel: 'arm64',
    notes: ['Needs glibc 2.39 or newer. Ubuntu 24.04 and Debian 13 can run this build.'],
  },
  // v0.8.2 muninn binary is thin Mach-O 64-bit ARM64 (MH_MAGIC_64, cputype
  // 0x0100000c), not a universal/fat binary. Keep this wording until a
  // universal or Intel build actually ships.
  'macos-aarch64': {
    os: 'macos',
    arch: 'aarch64',
    title: 'macOS',
    archLabel: 'Apple Silicon only',
    notes: [
      'Apple Silicon only (Intel Macs not supported). This build is not notarized, so Gatekeeper may warn on first open.',
    ],
  },
  'macos-x86_64': {
    os: 'macos',
    arch: 'x86_64',
    title: 'macOS',
    archLabel: 'Intel',
    notes: ['This build is not notarized, so Gatekeeper may warn on first open.'],
  },
  'windows-x86_64': {
    os: 'windows',
    arch: 'x86_64',
    title: 'Windows',
    archLabel: 'x86_64',
    notes: ['This build is not Authenticode-signed, so SmartScreen may warn on first run.'],
  },
  'windows-aarch64': {
    os: 'windows',
    arch: 'aarch64',
    title: 'Windows',
    archLabel: 'arm64',
    notes: ['This build is not Authenticode-signed, so SmartScreen may warn on first run.'],
  },
};

const CARD_ORDER = [
  'linux-x86_64',
  'linux-aarch64',
  'macos-aarch64',
  'macos-x86_64',
  'windows-x86_64',
  'windows-aarch64',
];

function assetRank(asset: ReleaseAsset): number {
  if (asset.kind === 'archive') return 0;
  if (asset.kind === 'deb') return 1;
  return 2;
}

export function groupCards(assets: ReleaseAsset[]): OsCard[] {
  const byId = new Map<string, ReleaseAsset[]>();
  for (const asset of assets) {
    if (!asset.cardId) continue;
    const list = byId.get(asset.cardId) ?? [];
    list.push(asset);
    byId.set(asset.cardId, list);
  }
  const ids = [
    ...CARD_ORDER.filter((id) => byId.has(id)),
    ...[...byId.keys()].filter((id) => !CARD_ORDER.includes(id)).sort(),
  ];
  return ids.map((id) => {
    const meta = CARD_META[id] ?? {
      os: 'other' as const,
      arch: id,
      title: id,
      archLabel: '',
      notes: [],
    };
    const grouped = (byId.get(id) ?? [])
      .slice()
      .sort((a, b) => assetRank(a) - assetRank(b) || a.name.localeCompare(b.name));
    return { id, ...meta, assets: grouped };
  });
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let value = bytes / 1024;
  for (let i = 0; i < units.length; i++) {
    if (value < 1024 || i === units.length - 1) {
      const n = value.toFixed(1).replace(/\.0$/, '');
      return `${n} ${units[i]}`;
    }
    value /= 1024;
  }
  return `${bytes} B`;
}

export function formatPublishedAt(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return new Intl.DateTimeFormat('en-US', {
    month: 'long',
    day: 'numeric',
    year: 'numeric',
    timeZone: 'UTC',
  }).format(date);
}

export function cosignIdentity(tag: string): string {
  return `https://github.com/${RELEASE_REPO}/.github/workflows/release.yml@refs/tags/${tag}`;
}

export function sha256sumCommand(asset: ReleaseAsset): string | null {
  if (!asset.sha256) return null;
  return `echo "${asset.sha256}  ${asset.name}" | sha256sum -c`;
}

export function cosignVerifyCommand(tag: string): string {
  return [
    'cosign verify-blob checksums.txt \\',
    '  --bundle checksums.txt.sigstore.json \\',
    `  --certificate-oidc-issuer ${COSIGN_OIDC_ISSUER} \\`,
    `  --certificate-identity ${cosignIdentity(tag)}`,
  ].join('\n');
}

export function normalizeRelease(raw: ReleaseSnapshot, source: ReleaseSource): LatestRelease {
  const checksums = parseChecksums(raw.checksumsTxt);
  const assets: ReleaseAsset[] = raw.assets.map((asset) => {
    const classified = classifyAsset(asset.name);
    const fromFile = checksums.get(asset.name) ?? null;
    const fromDigest = shaFromDigest(asset.digest, asset.sha256);
    return {
      name: asset.name,
      size: asset.size,
      url: asset.url,
      sha256: fromFile ?? fromDigest,
      kind: classified.kind,
      cardId: classified.cardId,
    };
  });
  return {
    source,
    tag: raw.tag,
    name: raw.name || raw.tag,
    htmlUrl: raw.htmlUrl,
    publishedAt: raw.publishedAt,
    body: raw.body,
    assets,
    checksumsUrl: assets.find((a) => a.kind === 'checksums')?.url ?? null,
    sigstoreUrl: assets.find((a) => a.kind === 'sigstore')?.url ?? null,
    checksumsTxt: raw.checksumsTxt,
    cards: groupCards(assets),
    cosignOidcIssuer: COSIGN_OIDC_ISSUER,
    cosignCertificateIdentity: cosignIdentity(raw.tag),
    cosignCertificateIdentityRegexp: COSIGN_IDENTITY_REGEXP,
  };
}

function githubToken(): string | undefined {
  const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  return token && token.length > 0 ? token : undefined;
}

function apiUrl(): string {
  return process.env.MUNINN_RELEASES_API_URL || DEFAULT_API;
}

function forceSnapshot(): boolean {
  const flag = process.env.MUNINN_RELEASES_SNAPSHOT;
  return flag === '1' || flag === 'true';
}

interface GithubReleaseAsset {
  name: string;
  size: number;
  browser_download_url: string;
  content_type?: string;
  digest?: string;
}

interface GithubRelease {
  tag_name: string;
  name?: string;
  html_url: string;
  published_at: string;
  body?: string;
  assets?: GithubReleaseAsset[];
}

async function fetchWithTimeout(url: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers);
  headers.set('User-Agent', 'muninn-site-build');
  const token = githubToken();
  if (token) headers.set('Authorization', `Bearer ${token}`);
  const signal = init.signal ?? AbortSignal.timeout(FETCH_TIMEOUT_MS);
  return fetch(url, { ...init, headers, signal, redirect: 'follow' });
}

async function fetchLatestFromApi(): Promise<ReleaseSnapshot> {
  const response = await fetchWithTimeout(apiUrl(), {
    headers: {
      Accept: 'application/vnd.github+json',
      'X-GitHub-Api-Version': '2022-11-28',
    },
  });
  if (!response.ok) {
    throw new Error(`GitHub Releases API returned ${response.status}`);
  }
  const data = (await response.json()) as GithubRelease;
  if (!data || typeof data.tag_name !== 'string' || !Array.isArray(data.assets)) {
    throw new Error('GitHub Releases API returned an unexpected payload');
  }
  const assets: SnapshotAsset[] = data.assets.map((asset) => ({
    name: asset.name,
    size: asset.size,
    url: asset.browser_download_url,
    contentType: asset.content_type,
    digest: asset.digest,
    sha256: shaFromDigest(asset.digest),
  }));
  const checksumsAsset = assets.find((asset) => asset.name === 'checksums.txt');
  let checksumsTxt = snapshot.checksumsTxt;
  if (checksumsAsset) {
    try {
      const checksumsResponse = await fetchWithTimeout(checksumsAsset.url, {
        headers: { Accept: 'application/octet-stream' },
      });
      if (checksumsResponse.ok) {
        const text = await checksumsResponse.text();
        if (parseChecksums(text).size > 0) checksumsTxt = text.endsWith('\n') ? text : `${text}\n`;
      }
    } catch {
      if (data.tag_name !== snapshot.tag) checksumsTxt = '';
    }
  }
  return {
    tag: data.tag_name,
    name: data.name || data.tag_name,
    htmlUrl: data.html_url,
    publishedAt: data.published_at,
    body: data.body || '',
    assets,
    checksumsTxt,
  };
}

export async function loadLatestRelease(log: LoaderLog = silent): Promise<LatestRelease> {
  if (forceSnapshot()) {
    log.info('Using the committed GitHub Releases snapshot (MUNINN_RELEASES_SNAPSHOT is set).');
    return normalizeRelease(snapshot, 'snapshot');
  }
  try {
    const live = await fetchLatestFromApi();
    const release = normalizeRelease(live, 'api');
    log.info(`Loaded ${release.tag} from the GitHub Releases API (${release.assets.length} assets).`);
    return release;
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    log.warn(`GitHub Releases API unavailable (${reason}); using the committed snapshot of ${snapshot.tag}.`);
    return normalizeRelease(snapshot, 'snapshot');
  }
}
