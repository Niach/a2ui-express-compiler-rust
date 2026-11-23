# Empty state

Request: The projects page of an app with no projects yet: an empty state with a folder icon, the title 'No projects yet', a one-sentence explanation, and a 'Create project' action.

## Sample A

```
Page title="Projects"
  EmptyState icon="folder" title="No projects yet" description="Projects help you organize your work, so create your first one to get started." actionLabel="Create project" size="page" on=[press→event create_project]
```

## Sample B

```
Page title="Projects"
  EmptyState icon="folder" title="No projects yet" description="Projects you create will show up here, so you can keep related work together." actionLabel="Create project" size="page" on=[press→event create_project]
```
