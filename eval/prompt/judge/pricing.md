# Pricing plans

Request: A pricing page: a monthly / yearly billing toggle, then three plan cards (Free $0, Pro $12, Team $30 per user), each with a short tagline, a list of four features, and a 'Choose plan' button; the Pro card is highlighted as recommended.

## Sample A

```
Page title="Pricing" subtitle="Simple plans that scale with you"
  Stack direction="vertical" gap="lg" align="center"
    Segmented items[2: "Monthly" | "Yearly"] type="single" value=$/billing on=[change→event set_billing {billing: $/billing}]
    Grid columns={base: 1, md: 3} gap="md"
      Card title="Free" description="For trying things out."
        Heading text="$0" level="h2"
        Text text="per user / month" variant="caption"
        List
          Row title="1 project" icon="check"
          Row title="Community support" icon="check"
          Row title="1 GB storage" icon="check"
          Row title="Basic analytics" icon="check"
        Button label="Choose plan" variant="outline" on=[press→event choose_plan {plan: "free", billing: $/billing}]
      Card title="Pro" description="For growing professionals." style{borderWidth, borderColor}
        Badge text="Recommended" variant="default" icon="star"
        Heading text="$12" level="h2"
        Text text="per user / month" variant="caption"
        List
          Row title="Unlimited projects" icon="check"
          Row title="Priority email support" icon="check"
          Row title="100 GB storage" icon="check"
          Row title="Advanced analytics" icon="check"
        Button label="Choose plan" variant="default" on=[press→event choose_plan {plan: "pro", billing: $/billing}]
      Card title="Team" description="For teams that work together."
        Heading text="$30" level="h2"
        Text text="per user / month" variant="caption"
        List
          Row title="Everything in Pro" icon="check"
          Row title="Shared workspaces" icon="check"
          Row title="Roles and permissions" icon="check"
          Row title="Single sign-on" icon="check"
        Button label="Choose plan" variant="outline" on=[press→event choose_plan {plan: "team", billing: $/billing}]
[data model] {"billing":"monthly"}
```

## Sample B

```
Page title="Pricing" subtitle="Simple plans that grow with you"
  Stack gap="lg"
    Segmented items[2: "Monthly" | "Yearly"] value=$/billing style{alignSelf}
    Grid columns={base: 1, md: 3} gap="md"
      Card title="Free" description="For trying things out"
        Stack gap="md"
          Heading level="h2" text="$0"
          List divided=true
            Row icon="check" title="1 project"
            Row icon="check" title="Community support"
            Row icon="check" title="1 GB storage"
            Row icon="check" title="Basic analytics"
          Button label="Choose plan" variant="outline" on=[press→event choosePlan {plan: "free", billing: $/billing}]
      Card title="Pro" description="For growing professionals" style{borderColor, borderWidth}
        Stack gap="md"
          Badge text="Recommended" icon="star"
          Heading level="h2" text=cond(if=eq(a=$/billing, b="yearly"), then="$10 / month", else="$12 / month")
          List divided=true
            Row icon="check" title="Unlimited projects"
            Row icon="check" title="Priority support"
            Row icon="check" title="100 GB storage"
            Row icon="check" title="Advanced analytics"
          Button label="Choose plan" on=[press→event choosePlan {plan: "pro", billing: $/billing}]
      Card title="Team" description="For collaborating teams"
        Stack gap="md"
          Heading level="h2" text=cond(if=eq(a=$/billing, b="yearly"), then="$25 / user / month", else="$30 / user / month")
          List divided=true
            Row icon="check" title="Everything in Pro"
            Row icon="check" title="Shared workspaces"
            Row icon="check" title="Roles and permissions"
            Row icon="check" title="SSO and audit log"
          Button label="Choose plan" variant="outline" on=[press→event choosePlan {plan: "team", billing: $/billing}]
[data model] / = {"billing":"monthly"}
```
