# Edit profile

Request: An edit-profile form prefilled with the current user (Ada Lovelace, ada@example.com, bio 'Analyst of engines', timezone Europe/London): avatar with a 'Change photo' action, display name, email, a bio textarea, a timezone select, and Save / Cancel.

## Sample A

```
Page title="Edit profile"
  Form name="profile"
    Stack
      Avatar name={"@path":"/form/name"}
      Button label="Change photo" variant="outline"
    Input label="Display name" value={"@path":"/form/name"} name="name"
    Input label="Email" value={"@path":"/form/email"} name="email" type="email"
    Textarea label="Bio" value={"@path":"/form/bio"} name="bio"
    Select label="Timezone" value={"@path":"/form/timezone"} name="timezone" options[7]
    FormActions
[data model] {"form":{"name":"Ada Lovelace","email":"ada@example.com","bio":"Analyst of engines","timezone":"Europe/London"}}
```

## Sample B

```
Page title="Edit profile"
  Form name="profile"
    Stack
      Stack
        Avatar name={"@path":"/profile/name"}
        Button label="Change photo" variant="outline"
      Input label="Display name" value={"@path":"/profile/name"} name="name" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/profile/name"}}},"me]
      Input label="Email" value={"@path":"/profile/email"} name="email" type="email" checks=[{"condition":{"@call":"email","args":{"value":{"@path":"/profile/email"}}},"mess]
      Textarea label="Bio" value={"@path":"/profile/bio"} name="bio"
      Select label="Timezone" value={"@path":"/profile/timezone"} name="timezone" options[7]
      FormActions
[data model] /profile = {"name":"Ada Lovelace","email":"ada@example.com","bio":"Analyst of engines","timezone":"Europe/London"}
```
