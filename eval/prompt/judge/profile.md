# Edit profile

Request: An edit-profile form prefilled with the current user (Ada Lovelace, ada@example.com, bio 'Analyst of engines', timezone Europe/London): avatar with a 'Change photo' action, display name, email, a bio textarea, a timezone select, and Save / Cancel.

## Sample A

```
Page title="Edit profile"
  Form name="profile" on=[submit→event saveProfile {name: $/profile/name, email: $/profile/email, bio: $/profile/bio, timezone: $/profile/timezone}]
    Stack gap="lg"
      Stack direction="horizontal" gap="md" align="center"
        Avatar name=$/profile/name size="xl"
        Button label="Change photo" variant="outline" on=[press→event changePhoto]
      Input label="Display name" name="name" value=$/profile/name checks=[required(value=$/profile/name) "Enter a display name"]
      Input label="Email" name="email" type="email" value=$/profile/email checks=[email(value=$/profile/email) "Enter a valid email address"]
      Textarea label="Bio" name="bio" rows=4 value=$/profile/bio
      Select label="Timezone" name="timezone" value=$/profile/timezone options[7: "Europe/London" | "Europe/Paris" | "Europe/Berlin" | "America/New_York" | "America/Los_Angeles" | "Asia/Tokyo" | "Australia/Sydney"]
      FormActions submit="Save" cancel=true on=[cancel→event cancelEdit]
[data model] /profile = {"name":"Ada Lovelace","email":"ada@example.com","bio":"Analyst of engines","timezone":"Europe/London"}
```

## Sample B

```
Page title="Profile"
  Card title="Edit profile"
    Form name="profile" on=[submit→event save_profile {name: $/form/name, email: $/form/email, bio: $/form/bio, timezone: $/form/timezone}]
      Stack direction="horizontal" align="center" gap="md"
        Avatar name="Ada Lovelace" size="xl"
        Button label="Change photo" variant="outline" icon="ui-camera" on=[press→event change_photo]
      Input label="Display name" name="name" value=$/form/name checks=[required(value=$/form/name) "Name is required"]
      Input label="Email" name="email" type="email" value=$/form/email checks=[required(value=$/form/email) "Email is required", email(value=$/form/email) "Enter a valid email"]
      Textarea label="Bio" name="bio" rows=4 value=$/form/bio
      Select label="Timezone" name="timezone" value=$/form/timezone searchable=true options[9: "Europe/London" | "Europe/Paris" | "Europe/Berlin" | "America/New_York" | "America/Chicago" | "America/Los_Angeles" | "Asia/Tokyo" | "Australia/Sydney" | "UTC"]
      FormActions submit="Save" cancel=true on=[cancel→event cancel_profile]
[data model] {"form":{"name":"Ada Lovelace","email":"ada@example.com","bio":"Analyst of engines","timezone":"Europe/London"}}
```
