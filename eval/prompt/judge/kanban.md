# Kanban board

Request: A small kanban board: three columns (To do, Doing, Done), each with a header showing its card count and two or three task cards with a title and an assignee avatar.

## Sample A

```
Page title="Kanban board"
  Grid
    Stack
      Stack
        Heading text="To do"
        Badge text={"@call":"len","args":{"value":{"@path":"/todo"}}} variant="secondary"
      List
    Stack
      Stack
        Heading text="Doing"
        Badge text={"@call":"len","args":{"value":{"@path":"/doing"}}} variant="secondary"
      List
    Stack
      Stack
        Heading text="Done"
        Badge text={"@call":"len","args":{"value":{"@path":"/done"}}} variant="secondary"
      List
[template card: repeated once per item of its list]
  Card
    Stack
      Avatar name={"@path":"assignee"}
      Text text={"@path":"title"}
[data model] {"todo":[{"title":"Draft launch checklist","assignee":"Maya Chen"},{"title":"Review pricing page","assignee":"Omar Reyes"},{"title":"Update onboarding copy","assignee":"Lena Fischer"}],"doing":[{"title":"Build billing webhook","assignee":"Omar Reyes"},{"title":"Design empty states","assignee":"Priya Nair"}],"done":[{"title":"Set up CI pipeline","assignee":"Lena Fischer"},{"title":"Write API docs","assignee":"Maya Chen"},{"title":"Fix login redirect","assignee":"Priya Nair"}]}
```

## Sample B

```
Page title="Kanban"
  Grid
    Stack
      Stack
        Heading text="To do"
        Badge text="3" variant="secondary"
      Card
        Stack
          Text text="Draft onboarding copy"
          Avatar name="Ada Lovelace"
      Card
        Stack
          Text text="Set up CI pipeline"
          Avatar name="Grace Hopper"
      Card
        Stack
          Text text="Review pricing page"
          Avatar name="Alan Turing"
    Stack
      Stack
        Heading text="Doing"
        Badge text="2" variant="secondary"
      Card
        Stack
          Text text="Build login form"
          Avatar name="Linus Torvalds"
      Card
        Stack
          Text text="Fix upload bug"
          Avatar name="Margaret Hamilton"
    Stack
      Stack
        Heading text="Done"
        Badge text="2" variant="secondary"
      Card
        Stack
          Text text="Design system audit"
          Avatar name="Ada Lovelace"
      Card
        Stack
          Text text="Ship v1 landing page"
          Avatar name="Grace Hopper"
```
