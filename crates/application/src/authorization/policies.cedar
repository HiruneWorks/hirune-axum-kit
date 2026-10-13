forbid (principal, action, resource)
when { !principal.active || !principal.verified };

permit (principal, action in [Action::"read_account", Action::"manage_account"], resource)
when { principal == resource.owner };

permit (principal, action == Action::"read_organization", resource)
when { context.organization_member };

permit (principal, action == Action::"manage_organization", resource)
when { context.organization_member && principal == resource.owner };

permit (principal, action == Action::"administer_account", resource)
when { principal.administrator };
