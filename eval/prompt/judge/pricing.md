# Pricing plans

Request: A pricing page: a monthly / yearly billing toggle, then three plan cards (Free $0, Pro $12, Team $30 per user), each with a short tagline, a list of four features, and a 'Choose plan' button; the Pro card is highlighted as recommended.

## Sample A

```
Page title="Pricing" subtitle="Pick the plan that fits your team"
  Stack
    Segmented value={"@path":"/billing"} items[2]
    Grid
      Card title="Free" description="For trying things out"
        Stack
          Heading text="$0"
          List
            Row title="1 project" icon="check"
            Row title="Up to 3 members" icon="check"
            Row title="Community support" icon="check"
            Row title="1 GB storage" icon="check"
          Button label="Choose plan" variant="outline"
      Card title="Pro" description="For growing teams that ship fast"
        Stack
          Badge text="Recommended" variant="default" icon="star"
          Heading text={"@call":"cond","args":{"if":{"@call":"eq","args":{"a":{"@pa
          Text text={"@call":"cond","args":{"if":{"@call":"eq","args":{"a":{"@pa variant="muted"
          List
            Row title="Unlimited projects" icon="check"
            Row title="Up to 20 members" icon="check"
            Row title="Priority support" icon="check"
            Row title="100 GB storage" icon="check"
          Button label="Choose plan"
      Card title="Team" description="For organizations that need control"
        Stack
          Heading text={"@call":"cond","args":{"if":{"@call":"eq","args":{"a":{"@pa
          Text text={"@call":"cond","args":{"if":{"@call":"eq","args":{"a":{"@pa variant="muted"
          List
            Row title="Unlimited members" icon="check"
            Row title="SSO and audit log" icon="check"
            Row title="Dedicated support" icon="check"
            Row title="1 TB storage" icon="check"
          Button label="Choose plan" variant="outline"
[data model] / = {"billing":"monthly"}
```

## Sample B

```
Page title="Pricing" subtitle="Pick the plan that fits"
  Stack
    Segmented value={"@path":"/billing"} type="single" items[2]
    Grid
      Card
        Stack
          Stack
            Heading text="Free"
            Text text="For trying things out" variant="muted"
            Heading text="$0"
          Stack
            Text text="1 project"
            Text text="Community support"
            Text text="Basic analytics"
            Text text="1 GB storage"
          Button label="Choose plan" variant="outline"
      Card
        Stack
          Stack
            Badge text="Recommended" variant="default"
            Heading text="Pro"
            Text text="For growing professionals" variant="muted"
            Heading text={"@call":"cond","args":{"if":{"@call":"eq","args":{"a":{"@pa
            Text text="per user / month" variant="caption"
          Stack
            Text text="Unlimited projects"
            Text text="Priority email support"
            Text text="Advanced analytics"
            Text text="50 GB storage"
          Button label="Choose plan" variant="default"
      Card
        Stack
          Stack
            Heading text="Team"
            Text text="For collaborating teams" variant="muted"
            Heading text={"@call":"cond","args":{"if":{"@call":"eq","args":{"a":{"@pa
            Text text="per user / month" variant="caption"
          Stack
            Text text="Everything in Pro"
            Text text="Shared workspaces"
            Text text="Roles and permissions"
            Text text="Dedicated support"
          Button label="Choose plan" variant="outline"
[data model] {"billing":"monthly"}
```
