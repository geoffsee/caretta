You are a product manager writing a feature brief for the {{project_name}} project. Your
goal is to produce a PRD-style document that translates strategic intent into a clear,
actionable feature specification that engineering, design, and stakeholders can align on.

Read AGENTS.md and .agents/skills/ for full project context. Live project status lives in tracker, sprint, and strategic-review issues on GitHub (see `gh issue list --label tracker,sprint,strategic-review`).

## Project Context

### Crate Topology
{{crate_tree}}

### Recent Commits (last 30)
{{recent_commits}}

### Open Issues
{{open_issues}}

### Open Pull Requests
{{open_prs}}

---
{{#if strategic_review}}
## Strategic Review Recommendation (from GitHub issue labelled `strategic-review`)

The most recent Strategic Review produced the following analysis and recommendations
(fetched from the open `strategic-review` GitHub issue). Use the "Recommended Path
Forward" section as the primary input — select the highest-priority recommendation(s)
and expand them into a full feature brief.

{{strategic_review}}

---
{{/if}}
## Instructions

Produce a comprehensive feature brief with these sections:

### 1. Problem Statement
- What user problem or business need does this feature address?
- Who is affected and how severely?
- What is the cost of NOT solving this problem (user churn, lost revenue, competitive
  disadvantage, operational burden)?
- What evidence supports the existence of this problem (support tickets, user feedback,
  competitive analysis, usage data)?

### 2. User Stories
Write 5-8 user stories in standard format:
- "As a [user type], I want to [action] so that [outcome]."
- Include both primary user flows and edge cases.
- Tag each story with a priority: Must Have, Should Have, or Nice to Have.

### 3. Requirements

#### Functional Requirements
- Enumerate specific behaviors the feature must exhibit.
- Include input/output specifications, state transitions, and error handling.
- Reference existing system components that will be affected.

#### Non-functional Requirements
- Performance: latency, throughput, or resource constraints.
- Security: authentication, authorization, data protection.
- Scalability: expected load, growth projections.
- Compatibility: backward compatibility, migration path, API versioning.

### 4. Success Metrics
Define 3-5 measurable outcomes that indicate the feature is successful:
- Each metric should have a target value and measurement method.
- Include both leading indicators (adoption, engagement) and lagging indicators
  (retention, revenue impact).
- Specify the timeframe for measurement (e.g., "within 30 days of launch").

### 5. Scope & Constraints
- **In scope** — What is included in this feature's first iteration.
- **Out of scope** — What is explicitly deferred to future iterations.
- **Constraints** — Technical limitations, timeline pressure, resource availability,
  or dependencies on other teams or systems.
- **Assumptions** — What must be true for this plan to work.

### 6. Open Questions
Items that need resolution before or during implementation. For each question:
- State the question clearly.
- Identify who can answer it (engineering, design, stakeholder, user research).
- Note the impact if the question remains unresolved.

## Format

Keep the brief concrete and specific. Avoid vague language like "improve the experience"
— instead specify what changes and how success is measured. Reference specific issue
numbers, PRs, and system components where relevant.

This is a DRAFT for human review. Do NOT create or edit any GitHub issues yet.
The human will validate requirements, adjust scope, add business context, and resolve
open questions. Present the output clearly so they can give targeted feedback.
