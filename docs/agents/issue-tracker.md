# Issue tracker: GitHub

Issues and PRDs live as GitHub issues. Use the `gh` CLI for all operations.

## Conventions

- Create, read, comment on, label, assign, and close issues with `gh issue`.
- Infer the repository from `git remote -v`.
- PRs are not a triage request surface.
- Publishing to the issue tracker means creating a GitHub issue.

## Wayfinding operations

- A map is an issue labelled `wayfinder:map`.
- Tickets are GitHub sub-issues, labelled `wayfinder:research`,
  `wayfinder:prototype`, `wayfinder:grilling`, or `wayfinder:task`.
- If sub-issues are unavailable, use a map task list and add
  `Part of #<map>` to each ticket.
- Use native GitHub issue dependencies for blocking relationships.
- If dependencies are unavailable, add `Blocked by: #<issue>` to the ticket.
- An open, unblocked, unassigned child ticket is on the frontier.
- Claim a ticket by assigning it to `@me` before starting work.
- Resolve it by posting the answer, closing the ticket, and appending a linked
  one-line gist to the map's Decisions-so-far section.
