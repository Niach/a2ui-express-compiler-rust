# Team directory

Request: A team directory: a search field, a count of members, and a grid of six people, each with an avatar, name, role, and a 'Message' button.

## Sample A

```
Page title="Team directory"
  Stack gap="lg"
    Toolbar
      [slot trailing]
        Text variant="muted" text=concat(values=[len(value=$/people), " members"])
      Input name="query" label="Search" type="search" placeholder="Search people" value=$/query
    Grid columns={base: 1, sm: 2, lg: 3} children=[template person, once per item of $/people]
[template person: repeated once per item of its list]
  Card
    Stack gap="sm" align="center"
      Avatar size="lg" name=$name seed=$id
      Heading level="h3" text=$name
      Text variant="muted" text=$role
      Button label="Message" icon="message-square" variant="outline" on=[press→event message {personId: $id}]
[data model] / = {"query":"","people":[{"id":"p1","name":"Amelia Hart","role":"Engineering Manager"},{"id":"p2","name":"Daniel Okafor","role":"Senior Engineer"},{"id":"p3","name":"Priya Nair","role":"Product Designer"},{"id":"p4","name":"Lucas Meyer","role":"Product Manager"},{"id":"p5","name":"Sofia Rossi","role":"Data Analyst"},{"id":"p6","name":"Kenji Watanabe","role":"QA Engineer"}]}
```

## Sample B

```
Page title="Team"
  Card title="Team directory"
    Stack direction="vertical" gap="md"
      Input label="Search people" name="query" type="search" placeholder="Search by name or role" value=$/query
      Text text=concat(values=[len(value=$/people), " members"]) variant="muted"
      Grid columns={base: 1, sm: 2, lg: 3} gap="md" children=[template person, once per item of $/people]
[template person: repeated once per item of its list]
  Card
    Stack direction="vertical" gap="sm" align="center"
      Avatar name=$name size="lg"
      Text text=$name variant="title"
      Text text=$role variant="muted"
      Button label="Message" variant="outline" on=[press→event message {person: $name}]
[data model] {"query":"","people":[{"name":"Ava Chen","role":"Engineering Lead"},{"name":"Marcus Webb","role":"Product Designer"},{"name":"Priya Nair","role":"Backend Engineer"},{"name":"Tomas Silva","role":"Frontend Engineer"},{"name":"Hannah Brooks","role":"Product Manager"},{"name":"Kenji Sato","role":"Data Analyst"}]}
```
