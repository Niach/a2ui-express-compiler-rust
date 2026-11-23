# Contact form

Request: A contact form: name, email (validated as an email), a subject select (Sales, Support, Other), a message textarea (required, at most 500 characters), and a 'Send message' button.

## Sample A

```
Page title="Contact us"
  Form name="contact"
    Input label="Name" value={"@path":"/form/name"} name="name"
    Input label="Email" value={"@path":"/form/email"} name="email" type="email" checks=[{"condition":{"@call":"email","args":{"value":{"@path":"/form/email"}}},"message]
    Select label="Subject" value={"@path":"/form/subject"} name="subject" options[3]
    Textarea label="Message" value={"@path":"/form/message"} name="message" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/message"}}},"me, {"condition":{"@call":"length","args":{"value":{"@path":"/form/message"},"min":0]
    FormActions
```

## Sample B

```
Page title="Contact us"
  Form name="contact"
    Stack
      Input label="Name" value={"@path":"/contact/name"} name="name"
      Input label="Email" value={"@path":"/contact/email"} name="email" type="email" checks=[{"condition":{"@call":"email","args":{"value":{"@path":"/contact/email"}}},"mess]
      Select label="Subject" placeholder="Choose a subject" value={"@path":"/contact/subject"} name="subject" options[3]
      Textarea label="Message" value={"@path":"/contact/message"} name="message" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/contact/message"}}},, {"condition":{"@call":"length","args":{"value":{"@path":"/contact/message"},"max]
    FormActions
[data model] /contact = {"name":"","email":"","subject":"","message":""}
```
