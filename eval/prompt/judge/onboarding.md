# Onboarding step

Request: Step 2 of 3 of a workspace onboarding wizard: a step indicator (Account, Workspace, Invite), a workspace name field (required), a team size choice (Just me, 2-10, 11-50, 51+), a 'Use for work' switch, and Back / Continue buttons.

## Sample A

```
Page title="Create your workspace" subtitle="Step 2 of 3"
  Stepper steps[3: "Account" | "Workspace" | "Invite"] current=1
  Card title="Set up your workspace"
    Form name="workspace" on=[submit→event onboarding_continue {name: $/form/name, size: $/form/size, work: $/form/work}]
      Input label="Workspace name" name="name" placeholder="Acme Inc." value=$/form/name checks=[required(value=$/form/name) "Workspace name is required"]
      Radio label="Team size" name="size" orientation="horizontal" value=$/form/size options[4: "Just me" | "2-10" | "11-50" | "51+"]
      Switch label="Use for work" name="work" checked=$/form/work
      Toolbar
        Button label="Back" variant="outline" on=[press→event onboarding_back]
        Button label="Continue" submit=true
[data model] {"form":{"name":"","size":"solo","work":true}}
```

## Sample B

```
Page title="Set up your workspace" subtitle="Step 2 of 3"
  Stack gap="lg"
    Stepper steps[3: "Account" | "Workspace" | "Invite"] current=1
    Form name="workspace" on=[submit→event continue {name: $/workspace/name, size: $/workspace/size, work: $/workspace/work}]
      Card title="Workspace details" description="Tell us a little about your workspace."
        Stack gap="lg"
          Input name="workspaceName" label="Workspace name" placeholder="Acme Inc." value=$/workspace/name checks=[required(value=$/workspace/name) "Workspace name is required"]
          Radio name="teamSize" label="Team size" orientation="horizontal" value=$/workspace/size options[4: "Just me" | "2-10" | "11-50" | "51+"]
          Switch name="useForWork" label="Use for work" checked=$/workspace/work
          Stack direction="horizontal" justify="end" gap="sm"
            Button label="Back" variant="outline" on=[press→event back]
            Button label="Continue" submit=true
[data model] / = {"workspace":{"name":"","size":"solo","work":true}}
```
