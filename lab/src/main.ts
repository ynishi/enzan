import init, {
  Content,
  isEnzanError,
  maxTrials,
  type ContentSpec,
  type Issue,
  type LootReport,
} from "./pkg/enzan_wasm.js";
import sample from "../../crates/enzan-wasm/tests/fixtures/content.json";

function byId<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (element === null) {
    throw new Error(`index.html has no #${id}`);
  }
  return element as T;
}

const specBox = byId<HTMLTextAreaElement>("spec");
const status = byId<HTMLSpanElement>("status");
const issuesBox = byId<HTMLDivElement>("issues");
const tableSelect = byId<HTMLSelectElement>("table");
const levelInput = byId<HTMLInputElement>("level");
const seedInput = byId<HTMLInputElement>("seed");
const trialsInput = byId<HTMLInputElement>("trials");
const errorBox = byId<HTMLDivElement>("error");
const reportBox = byId<HTMLDivElement>("report");

/** A number for a person: a few significant digits, no trailing zeros. */
function num(n: number): string {
  return Number.isInteger(n) ? String(n) : String(Number(n.toPrecision(5)));
}

function percent(part: number, whole: number): string {
  return whole === 0 ? "" : `${((100 * part) / whole).toFixed(2)}%`;
}

/** A table of text cells; the first column is left-aligned. */
function table(headers: string[], rows: string[][]): HTMLTableElement {
  const element = document.createElement("table");
  const head = element.createTHead().insertRow();
  headers.forEach((text, i) => {
    const th = document.createElement("th");
    th.textContent = text;
    if (i === 0) th.className = "left";
    head.appendChild(th);
  });
  const body = element.createTBody();
  for (const row of rows) {
    const tr = body.insertRow();
    row.forEach((text, i) => {
      const td = tr.insertCell();
      td.textContent = text;
      if (i === 0) td.className = "left";
    });
  }
  return element;
}

function heading(text: string): HTMLHeadingElement {
  const h = document.createElement("h3");
  h.textContent = text;
  return h;
}

/** The text of the box as JSON, or why it is not. */
function parseSpec(): { ok: true; value: unknown } | { ok: false; message: string } {
  try {
    return { ok: true, value: JSON.parse(specBox.value) };
  } catch (e) {
    return { ok: false, message: e instanceof Error ? e.message : String(e) };
  }
}

function showStatus(text: string, good: boolean): void {
  status.textContent = text;
  status.className = good ? "ok" : "bad";
}

function showIssues(issues: Issue[]): void {
  issuesBox.replaceChildren();
  if (issues.length > 0) {
    issuesBox.appendChild(
      table(
        ["code", "at", "message"],
        issues.map((issue) => [issue.code, issue.at, issue.message]),
      ),
    );
  }
}

/** The ids of the drop tables in the content, in the select. */
function fillTables(spec: ContentSpec): void {
  const chosen = tableSelect.value;
  tableSelect.replaceChildren();
  for (const t of spec.dropTables) {
    const option = document.createElement("option");
    option.value = String(t.id);
    option.textContent = String(t.id);
    tableSelect.appendChild(option);
  }
  if (Array.from(tableSelect.options).some((o) => o.value === chosen)) {
    tableSelect.value = chosen;
  }
}

/** Checks the content in the box; returns it when there is nothing to fix. */
function validate(): ContentSpec | null {
  const parsed = parseSpec();
  if (!parsed.ok) {
    showStatus(`Not JSON: ${parsed.message}`, false);
    showIssues([]);
    return null;
  }
  // `validate` takes anything; the cast is for the type the generated
  // signature asks for, and the check is the library's.
  const spec = parsed.value as ContentSpec;
  const issues = Content.validate(spec);
  showIssues(issues);
  if (issues.length > 0) {
    showStatus(`${issues.length} problem(s)`, false);
    return null;
  }
  showStatus("Valid", true);
  fillTables(spec);
  return spec;
}

function showError(e: unknown): void {
  if (isEnzanError(e)) {
    errorBox.textContent = `${e.code}${e.at === "" ? "" : ` at ${e.at}`}: ${e.message}`;
  } else {
    errorBox.textContent = String(e);
  }
}

function renderReport(report: LootReport): void {
  reportBox.replaceChildren(
    heading(`${report.trials} trials, ${report.items} items`),
    heading("Rarities"),
    table(
      ["rarity", "count", "share of items"],
      report.rarities.map((r) => [String(r.id), String(r.count), percent(r.count, report.items)]),
    ),
    heading("Affixes"),
    table(
      ["affix", "count", "min", "mean", "p50", "p90", "p99", "max"],
      report.affixes.map((a) => {
        const v = a.value;
        return [
          String(a.id),
          String(a.count),
          ...(v === null
            ? ["", "", "", "", "", ""]
            : [v.min, v.mean, v.p50, v.p90, v.p99, v.max].map(num)),
        ];
      }),
    ),
    heading("Affixes per item"),
    table(
      ["affixes", "items", "share of items"],
      report.affixCount.map((b) => [String(b.value), String(b.count), percent(b.count, report.items)]),
    ),
    heading("Items per defeat"),
    table(
      ["items", "defeats", "share of defeats"],
      report.drops.map((b) => [String(b.value), String(b.count), percent(b.count, report.trials)]),
    ),
  );
}

function run(): void {
  errorBox.textContent = "";
  const parsed = parseSpec();
  if (!parsed.ok) {
    errorBox.textContent = `Not JSON: ${parsed.message}`;
    return;
  }
  // `BigInt("")` is 0n, so an empty seed would run as seed 0 without a word.
  const seedText = seedInput.value.trim();
  if (seedText === "") {
    errorBox.textContent = "The seed is empty";
    return;
  }
  try {
    // The content lives for this Run only. Not `using`: the package sets
    // `Symbol.dispose` only where the browser has it.
    const content = Content.load(parsed.value as ContentSpec);
    try {
      renderReport(
        content.measureLoot({
          table: Number(tableSelect.value),
          itemLevel: Number(levelInput.value),
          seed: BigInt(seedText),
          trials: Number(trialsInput.value),
        }),
      );
    } finally {
      content.free();
    }
  } catch (e) {
    reportBox.replaceChildren();
    showError(e);
  }
}

async function main(): Promise<void> {
  await init();
  specBox.value = JSON.stringify(sample, null, 2);
  // The limit comes from the core through the interface.
  trialsInput.max = String(maxTrials());
  specBox.addEventListener("input", validate);
  byId("validate").addEventListener("click", validate);
  byId("run").addEventListener("click", run);
  validate();
}

void main();
