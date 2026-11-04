# PRD: Potagia

| Field | Value |
|---|---|
| **Author** | Rubén Pozo Molina |
| **Status** | In review |
| **Version** | 0.2 |
| **Date** | 2026-10-04 |
| **Last updated** | 2026-11-04 |

## 1. Summary

Potagia is a proposed web application that provides management applications from an existing database model without requiring users to write application code. Administrators configure access and select the data to expose; users interact with that data through generated interfaces.

This document describes a proposed first release, not implemented functionality. Proposed scope and targets require validation.

## 2. Context and problem

- **Problem:** Building and maintaining separate management applications for existing databases is expensive and complex.
- **Current approach:** Each application typically requires its own development and ongoing maintenance.
- **Why now:** Demand for customized applications and automation creates an opportunity to provide reusable management interfaces without application programming.

## 3. Goals and non-goals

### Goals

- Generate applications from existing database models without application programming.
- Manage users and permissions with role-based access control (RBAC).
- Protect access to applications and data.
- Provide human-readable URLs, support multiple database types and languages, and deliver responsive interactions.

### Non-goals for the first release

- Creating or modeling databases.
- Plugins, custom code, or user-developed functionality.
- Automatically exposing every database object or granting public access by default.

## 4. Users and use cases

### User profiles

| Profile | Description | Primary need |
|---|---|---|
| Administrator | Manages configuration, users and access. | Control applications and permissions. |
| Developer | Configures an application from an existing database model without writing application code. | Make the required data available to authorized users. |
| Editor | Works with data according to assigned permissions. | View and maintain records. |
| Public visitor | An unauthenticated user with access only to explicitly published data. | View public information. |

These are example profiles; permissions are assigned through roles rather than assumed from a profile name.

### User stories

- **US-001:** As an administrator, I want to configure a database connection so that an application can use an existing model.
- **US-002:** As a developer, I want to select database objects and configure generated views so that users can work without a custom application.
- **US-003:** As an administrator, I want to assign roles and permissions so that users can only access authorized data and operations.
- **US-004:** As an editor, I want to browse, search and edit permitted records so that I can maintain information.
- **US-005:** As a public visitor, I want to view explicitly published information without signing in.
- **US-006:** As a user, I want to choose a supported interface language so that I can use the application comfortably.

## 5. Functional requirements

Priority: **M** = Must, **S** = Should, **C** = Could, **W** = Won't in this release. These priorities describe the proposed release scope.

| ID | Requirement | Priority | Notes |
|---|---|---|---|
| RF-01 | Configure and test a connection to an existing supported database and inspect its schema. | M | Supported engines remain to be decided; credentials must not be exposed. |
| RF-02 | Select supported tables and fields and generate an application with list and detail views. | M | Only explicitly selected objects are exposed. |
| RF-03 | Authenticate users and manage accounts, roles and permissions. | M | Permissions cover application, object and operation; deny by default. |
| RF-04 | Allow authorized users to create, read, update and delete records. | M | Respect database constraints and permissions; report validation errors. |
| RF-05 | Provide pagination and search or filtering for authorized records. | M | Supported field types and filter operators are to be defined. |
| RF-06 | Publish selected data for unauthenticated read-only access. | S | Publication must be explicit; no public write access. |
| RF-07 | Provide stable, human-readable application and resource URLs. | S | URLs do not bypass authorization. |
| RF-08 | Provide a localized interface and a language selector. | S | Initial languages and translation coverage are to be decided. |
| RF-09 | Support more than one database engine through a common connection interface. | S | The first-release engine list is an open decision. |
| RF-10 | Create or modify database schemas, install plugins or run user-defined application code. | W | Outside the first-release scope. |

## 6. Non-functional requirements

Targets below are proposed acceptance thresholds, not measured results; workload, deployment environment and feasibility must be agreed before release.

| ID | Category | Requirement |
|---|---|---|
| RNF-01 | Performance | Under an agreed representative workload, 95% of list and detail requests complete within 2 seconds, excluding network latency; measure server response time. |
| RNF-02 | Security | Enforce authorization on the server for every data operation, use TLS in deployment, protect stored credentials, and never log secrets. |
| RNF-03 | Accessibility | Target WCAG 2.2 AA for generated user flows, including keyboard navigation and accessible form errors. |
| RNF-04 | Compatibility | Support the latest stable versions of Firefox, Chrome, Edge and Safari on desktop; supported mobile browsers are to be agreed. |
| RNF-05 | Scalability | Paginate large result sets and avoid loading entire tables; validate concurrent-user capacity against an agreed workload before release. |
| RNF-06 | Reliability | Failed writes must not be reported as successful; preserve database integrity and show actionable errors without exposing sensitive details. |
| RNF-07 | Observability | Record operational errors and security-relevant access events without logging credentials or sensitive record contents. |

## 7. Acceptance criteria

### RF-01: Database connection and schema inspection

- [ ] Given valid connection details for a supported engine, when an administrator tests the connection, then the result indicates success and accessible schema objects can be listed.
- [ ] Given invalid or unreachable connection details, when the connection is tested, then an actionable error is shown without revealing credentials.

### RF-02: Generated application

- [ ] Given a successfully inspected schema, when a developer selects a supported table and fields and publishes the application, then authorized users can open its list and detail views.
- [ ] Given an unselected table or field, when a user requests it through the interface or API, then its data is not returned.

### RF-03: Accounts and authorization

- [ ] Given an account with an assigned role, when the user signs in, then only operations granted to that role are available.
- [ ] Given a user without permission, when they request a protected operation directly, then the server denies it and does not return protected data.

### RF-04: Record management

- [ ] Given the relevant permissions, when an editor creates, updates or deletes a valid record, then the change is persisted and reflected in the view.
- [ ] Given invalid input or a database constraint violation, when a write is attempted, then the operation fails and the user receives an appropriate error.

### RF-05: Browse and search

- [ ] Given more records than fit on one page, when an authorized user navigates pages, then each page returns only its requested records.
- [ ] Given a supported filter, when an authorized user applies it, then only matching records they may read are returned.

### RF-06: Public data

- [ ] Given data explicitly published for public reading, when a visitor opens its URL without signing in, then only the published fields and records are visible.
- [ ] Given unpublished data or a write request, when an unauthenticated visitor requests it, then access is denied.

### RF-07: URLs

- [ ] Given a published resource, when its human-readable URL is opened, then it resolves to the resource subject to the same authorization checks as other access paths.

### RF-08: Localization

- [ ] Given two configured supported languages, when a user changes language, then generated interface labels and messages appear in the selected language.

### RF-09: Database engines

- [ ] Given each engine selected for support, when its connection is configured and tested, then supported schema inspection and record operations pass the same acceptance scenarios.

## 8. Design and flows

- **Wireframes / mockups:** Not yet available; create and review before implementation of the generated interface.
- **Main flow:**
  1. An administrator configures and tests a database connection.
  2. A developer inspects the schema, selects objects and fields, and configures the application views.
  3. An administrator assigns roles and permissions and explicitly publishes any public data.
  4. An authorized user opens the application, browses or searches records, and performs permitted operations.
- **UX principles:** Make permissions and publication status visible to administrators; provide clear validation and empty states; keep generated views consistent and keyboard accessible.

## 9. Technical considerations

- **Current state:** The repository contains a Rust 2024 package with a minimal executable and no declared dependencies. It does not yet implement the web application.
- **Proposed architecture:** A web interface and server-side authorization layer use database-specific adapters to inspect schemas and execute permitted operations. Frameworks and adapter implementations are not yet selected.
- **Integrations and external APIs:** Existing databases are the primary integrations. No external API dependency has been selected.
- **Data model:** Application configuration includes connections, selected objects and fields, users, roles, permissions and publication settings. Business records remain in their source databases. Storage for configuration and secrets is to be decided.
- **Deployment:** Deployment topology, secret management, backups, TLS termination and operational requirements remain to be defined before release.
- **Schema changes:** Define how changes to source schemas are detected and how incompatible configuration is reported; do not silently expose new fields.

## 10. Success metrics

Baselines are unavailable. The targets below are proposals to validate with stakeholders and a representative pilot.

| Metric | Current value | Proposed target | Measurement |
|---|---|---|---|
| Time to first application | Unknown | At most 30 minutes for a representative existing schema | Timed pilot from connection setup to a usable authorized view. |
| Authorized task completion | Unknown | At least 90% of pilot users complete agreed browse and edit tasks without developer assistance | Observed pilot tasks and completion records. |
| Response time | Unknown | 95% of list and detail requests within 2 seconds | Server-side measurements under the agreed representative workload. |
| Access control | Unknown | Zero unauthorized reads or writes in the agreed acceptance suite | Automated authorization tests covering roles, direct requests and public access. |

## 11. Dependencies, risks and assumptions

### Dependencies

- Access to representative existing databases, schemas and test data.
- Supported database drivers and permissions for schema inspection and permitted operations.
- Decisions on authentication, secret storage, deployment and the initial database engines.
- Stakeholder review of scope, accessibility, target workload and pilot criteria.

### Risks

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| Unauthorized data exposure through generated views or public access | Medium | High | Deny by default, explicitly select fields, enforce server-side authorization and test direct requests. |
| Differences between database engines or unsupported schema types | High | High | Define a supported subset, validate adapters against representative schemas and communicate limitations. |
| Source schema changes break configured applications | Medium | High | Detect incompatibilities, report them to administrators and require review before exposing new fields. |
| Large tables produce slow queries | Medium | Medium | Paginate, use bounded queries and test with representative data volumes. |
| Credentials or sensitive data leak through logs or configuration | Medium | High | Use approved secret storage, restrict access and exclude secrets and record contents from logs. |

### Assumptions

- Customers already have a database and can grant the required connection privileges.
- Users can configure applications without writing application code; they do not need to modify the database schema through Potagia.
- Public access is optional and applies only to explicitly published data.
- The first release supports a documented subset of schema types and operations; the subset is not yet agreed.

## 12. Milestones and plan

Dates and ownership are not yet agreed; milestones are proposed in dependency order.

| Milestone | Deliverable | Estimated date |
|---|---|---|
| M1 | Approved scope, initial engine selection, security decisions and reviewed UX flows. | To be agreed |
| M2 | Database connection, schema inspection, generated read views and authorization validated against a representative database. | To be agreed |
| M3 | Permitted record management, agreed release features and acceptance, security and performance validation. | To be agreed |

## 13. Open questions

- [ ] Which database engines, versions, schema types and relationships are in the first release? Owner and due date: to be assigned.
- [ ] Which authentication mechanism, account lifecycle and permission granularity are required? Owner and due date: to be assigned.
- [ ] Which languages and mobile browsers must be supported initially? Owner and due date: to be assigned.
- [ ] Is anonymous read-only publication required for the first release, and must access be restricted by row? Owner and due date: to be assigned.
- [ ] What are the deployment model, workload, concurrency and data-volume targets? Owner and due date: to be assigned.
- [ ] Where are configuration and credentials stored, and how are backups and schema changes handled? Owner and due date: to be assigned.
- [ ] Which proposed metrics and thresholds will stakeholders approve for the pilot and release? Owner and due date: to be assigned.

## 14. Change history

| Version | Date | Author | Changes |
|---|---|---|---|
| 0.1 | 2026-10-04 | Rubén Pozo Molina | Initial draft. |
| 0.2 | 2026-11-04 | Junie | Completed draft requirements, acceptance criteria, risks and proposed release plan. |
