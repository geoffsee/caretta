# Revenue Sharing Agreement

This Revenue Sharing Agreement ("Agreement") sets out supplemental terms
for **large organizations** that put the caretta software or product
("caretta," the "Software") to material use. It sits **alongside** the
open-source licenses in `LICENSE-MIT` and `LICENSE-APACHE`; it does not
replace them for anyone it does not cover.

The intent is narrow and stated plainly: individuals, small teams,
hobbyists, students, researchers, and organizations under the revenue
threshold below keep the ordinary, permissive open-source grant with no
new obligation. Well-resourced organizations that build materially on this
work are asked to **make contact first** and to share a fair portion of the
revenue that the Software helps produce.

> **Not legal advice.** This document is a plain-language agreement, not a
> substitute for counsel. Both the Author and any Covered Organization
> should have their own lawyers review it before relying on it. Where this
> Agreement and an open-source license genuinely conflict as to a Covered
> Organization, the parties will resolve the conflict in a signed writing
> under §9.

## 1. Definitions

1. **"Author"** means the copyright holder(s) of caretta — the maintainer
   of `github.com/geoffsee/caretta` and successors in interest.
2. **"Software"** means caretta: its source code, compiled binaries,
   assets, configuration presets, workflow definitions, and accompanying
   documentation in this repository.
3. **"Organization"** means any legal entity — including a company,
   corporation, partnership, nonprofit, cooperative, agency, department, or
   other body — **whether governmental or non-governmental**, and any
   entity that controls, is controlled by, or is under common control with
   it.
4. **"Annual Revenue"** means an Organization's total gross revenue,
   receipts, appropriations, budget, or equivalent inflows from all
   sources over its most recent completed fiscal year, measured on a
   consolidated basis across affiliated entities. For a governmental body,
   Annual Revenue includes appropriated and operating budget.
5. **"Threshold"** means **five hundred thousand United States dollars
   (US $500,000)** in Annual Revenue.
6. **"Covered Organization"** means any Organization whose Annual Revenue
   equals or exceeds the Threshold **and** that engages in a Covered Use.
7. **"Covered Use"** means using, incorporating, deploying, distributing,
   offering as a service, or otherwise putting into production a
   **Significant Portion** of the Software.
8. **"Significant Portion"** means any of the following:
   - a substantial part of the Software's source code (as a guide, more
     than a *de minimis* excerpt — e.g., a whole crate, module, subsystem,
     workflow preset, or a meaningful fraction of a file's logic);
   - the Software, or a derivative of it, run in production, offered to
     third parties, or embedded in a product or service the Organization
     provides; or
   - use of the Software that is material to a product, service, revenue
     line, or operational function of the Organization.

   Isolated evaluation, testing, prototyping, security research, teaching,
   and casual or internal-only experimentation are **not** a Significant
   Portion.

## 2. Who this Agreement covers

1. This Agreement applies **only** to a **Covered Organization**.
2. If an Organization's Annual Revenue is **below the Threshold**, this
   Agreement imposes **no obligation** on it. It uses the Software under
   `LICENSE-MIT` or `LICENSE-APACHE` as it chooses, with nothing more owed.
3. Individuals acting in a personal capacity are never Covered
   Organizations.
4. Governmental and non-governmental bodies are treated identically: the
   only tests are the Threshold and whether the use is a Covered Use.

## 3. The core obligation: contact before Significant Portion use

A Covered Organization agrees, as a binding condition of a Covered Use,
that **before** it first puts a Significant Portion of the Software into a
Covered Use it will:

1. **Make contact** with the Author (see §7), identifying itself, the
   intended use, and the approximate scale of that use; and
2. **Negotiate in good faith** a revenue-sharing arrangement under §4
   before deriving material revenue or operational benefit from the
   Covered Use.

Making contact is the trigger for everything else. The Author's aim is to
be reachable and reasonable; the obligation is to reach out, not to obtain
permission before ordinary internal evaluation.

## 4. Revenue sharing

1. Upon contact, the Author and the Covered Organization will negotiate in
   good faith a **fair revenue-sharing arrangement** proportionate to the
   Covered Organization's use, scale, and the benefit the Software
   provides.
2. Unless the parties agree otherwise in writing, the arrangement will be
   documented in a short, signed **schedule** appended to or referencing
   this Agreement, stating the share, the measurement basis, the reporting
   cadence, and the term.
3. The Author may, at its sole discretion, **waive or reduce** the share —
   including to zero — for any Covered Organization, for a class of uses,
   or for a period. Common cases for waiver include public-interest,
   nonprofit, educational, disaster-response, and open-source downstream
   uses. A waiver is only effective if the Author gives it in writing.
4. Nothing here requires a Covered Organization to pay a share on revenue
   the Software did not help produce.

## 5. Good-faith determination of coverage

1. Whether a use is a **Significant Portion** or a **Covered Use** is
   judged on substance, not labels, and is resolved in **good faith**.
2. An Organization uncertain whether it is covered should treat §3 as
   cheap insurance: **make contact and ask.** A good-faith inquiry that
   turns out not to be a Covered Use costs the Organization nothing under
   this Agreement.
3. The Threshold is measured at the time a Covered Use begins and is
   re-tested at the start of each of the Organization's fiscal years for
   the duration of the Covered Use.

## 6. Relationship to the open-source licenses

1. The Software remains available under `LICENSE-MIT` and
   `LICENSE-APACHE`. This Agreement does not revoke, diminish, or
   retroactively alter any grant already made under those licenses.
2. This Agreement is an **additional, contractual commitment** that a
   Covered Organization makes when it undertakes a Covered Use. It is not a
   restriction on the rights of anyone this Agreement does not cover.
3. For everyone who is not a Covered Organization, the open-source licenses
   are the whole of the deal.

## 7. Contact

Make contact through the lowest-friction channel available:

- Open a **private inquiry** referencing this Agreement via the repository
  at `https://github.com/geoffsee/caretta` (a private security-style
  advisory, a direct message, or a dedicated issue as the repository's
  contact guidance directs); or
- Reach the Author at the contact address published in the repository
  metadata (`Cargo.toml` `repository`/`homepage`) or `README.md`.

A Covered Organization has satisfied the **contact** step of §3 when it has
sent a good-faith notice to one of these channels and allowed the Author a
reasonable period to respond.

## 8. Good faith, and what this Agreement is not

1. This Agreement is written to be **liberal and reasonable**, in the same
   spirit as this project's `COVENANT.md`: short, clear, and imposing
   obligations only where the stakes are real.
2. It is **not** a trap for casual users, small businesses, students, or
   researchers. It is **not** a clawback on the permissive licenses. It is
   **not** a demand for payment before an Organization knows whether the
   Software is even useful to it.
3. It **is** an ask, backed by contract, that large organizations —
   public or private — which build materially on this work start with a
   conversation and share fairly in the value it helps create.

## 9. Miscellaneous

1. **Amendment.** This Agreement is amended only by a human-authored commit
   to this file on the default branch, or by a signed writing between the
   Author and a specific Covered Organization for that Organization.
2. **Severability.** If any provision is held unenforceable, the rest
   remains in force, and the unenforceable provision is read down to the
   narrowest change that makes it enforceable.
3. **No waiver by inaction.** The Author's delay or failure to enforce any
   provision is not a waiver of it.
4. **Precedence.** As to a Covered Organization, and only as to that
   Organization, where this Agreement and an open-source license conflict,
   the parties will resolve the conflict in a signed writing; absent one,
   the open-source license governs the rights it grants and this Agreement
   governs the supplemental revenue-sharing commitment.
5. **Governing terms.** The governing law and venue for a specific
   revenue-sharing arrangement are set in that arrangement's §4 schedule.
</content>
</invoke>
