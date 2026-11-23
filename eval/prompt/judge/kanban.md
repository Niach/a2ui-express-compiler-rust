# Kanban board

Request: A small kanban board: three columns (To do, Doing, Done), each with a header showing its card count and two or three task cards with a title and an assignee avatar.

## Sample A

```
Page title="Kanban board"
  Grid columns={base: 1, md: 3} gap="md"
    Section title="To do" count=3 tint="neutral"
      Row title="Write onboarding copy" surface="card"
        [slot leading]
          Avatar name="Ada Lovelace" size="sm"
      Row title="Set up CI pipeline" surface="card"
        [slot leading]
          Avatar name="Grace Hopper" size="sm"
      Row title="Draft release notes" surface="card"
        [slot leading]
          Avatar name="Alan Turing" size="sm"
    Section title="Doing" count=2 tint="info"
      Row title="Design settings page" surface="card"
        [slot leading]
          Avatar name="Linus Torvalds" size="sm"
      Row title="Fix login redirect bug" surface="card"
        [slot leading]
          Avatar name="Margaret Hamilton" size="sm"
    Section title="Done" count=3 tint="success"
      Row title="Create project repo" surface="card"
        [slot leading]
          Avatar name="Ada Lovelace" size="sm"
      Row title="Pick a color palette" surface="card"
        [slot leading]
          Avatar name="Grace Hopper" size="sm"
      Row title="Kickoff meeting" surface="card"
        [slot leading]
          Avatar name="Alan Turing" size="sm"
```

## Sample B

```
Page title="Kanban board"
  Grid columns={base: 1, md: 3} gap="md"
    Card
      Section title="To do" count=len(value=$/todo)
        List gap="sm" children=[template taskCard, once per item of $/todo]
    Card
      Section title="Doing" count=len(value=$/doing)
        List gap="sm" children=[template taskCard, once per item of $/doing]
    Card
      Section title="Done" count=len(value=$/done)
        List gap="sm" children=[template taskCard, once per item of $/done]
[template taskCard: repeated once per item of its list]
  Row title=$title surface="card"
    [slot leading]
      Avatar name=$assignee size="sm"
[data model] {"todo":[{"title":"Draft launch email","assignee":"Maya Chen"},{"title":"Review pricing page","assignee":"Omar Haddad"},{"title":"Plan user interviews","assignee":"Lena Fischer"}],"doing":[{"title":"Build onboarding flow","assignee":"Priya Nair"},{"title":"Fix login redirect bug","assignee":"Tom Becker"}],"done":[{"title":"Set up CI pipeline","assignee":"Sam Okafor"},{"title":"Write release notes","assignee":"Maya Chen"},{"title":"Migrate database","assignee":"Lena Fischer"}]}
```
