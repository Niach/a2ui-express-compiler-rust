# Contact form

Request: A contact form: name, email (validated as an email), a subject select (Sales, Support, Other), a message textarea (required, at most 500 characters), and a 'Send message' button.

## Sample A

```
Page title="Contact us"
  Card title="Send us a message" description="We usually reply within one business day."
    Form name="contact" on=[submit→event sendMessage {name: $/contact/name, email: $/contact/email, subject: $/contact/subject, message: $/contact/message}]
      Input label="Name" name="name" value=$/contact/name placeholder="Your name"
      Input label="Email" name="email" type="email" value=$/contact/email placeholder="you@example.com" checks=[email(value=$/contact/email) "Enter a valid email address."]
      Select label="Subject" name="subject" value=$/contact/subject options[3: "Sales" | "Support" | "Other"]
      Textarea label="Message" name="message" rows=5 value=$/contact/message placeholder="How can we help?" checks=[required(value=$/contact/message) "Message is required.", length(value=$/contact/message, min=0, max=500) "Message must be at most 500 characters."]
      FormActions submit="Send message"
[data model] / = {"contact":{"name":"","email":"","subject":"Sales","message":""}}
```

## Sample B

```
Page title="Contact"
  Card title="Contact us"
    Form name="contact" on=[submit→event send_message {name: $/form/name, email: $/form/email, subject: $/form/subject, message: $/form/message}]
      Input label="Name" name="name" value=$/form/name
      Input label="Email" name="email" type="email" value=$/form/email checks=[email(value=$/form/email) "Enter a valid email address"]
      Select label="Subject" name="subject" value=$/form/subject options[3: "Sales" | "Support" | "Other"]
      Textarea label="Message" name="message" rows=5 value=$/form/message checks=[required(value=$/form/message) "Message is required", length(value=$/form/message, min=0, max=500) "Message must be at most 500 characters"]
      FormActions submit="Send message"
[data model] {"form":{"name":"","email":"","subject":"Sales","message":""}}
```
