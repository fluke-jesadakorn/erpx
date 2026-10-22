export function betaDestinationAllowed(destination: string, api: boolean) {
    if (api && /^people\/organization(?:\/.*)?$/.test(destination))
        return true;
    if (!api && destination === "people/organization")
        return true;
    if (api && /^messaging(?:\/.*)?$/.test(destination))
        return true;
    return api
        ? /^(beta(?:\/.*)?|billing(?:\/.*)?|flows\/(?:expense|request|staff_task)\/[^/]+|requests(?:\/.*)?|service-teams(?:\/.*)?|handbook(?:\/.*)?|work\/(?:summary|card-preferences)|ai-providers(?:\/.*)?|expenses(?:\/.*)?|people\/directory(?:\/[^/]+(?:\/(?:status|grant|revoke))?)?|posts\/[^/]+(?:\/(?:task|task-source|ai-drafts|revisions))?|staff-tasks(?:\/.*)?|approvals\/[^/]+\/reroute|staff\/(?:people|settings|levels(?:\/[^/]+)?|engagements\/[^/]+\/level|departments\/[^/]+\/head|notifications(?:\/[^/]+\/read)?)|branding(?:\/.*)?|uploads(?:\/[^/]+\/content)?|attachments\/[^/]+\/(?:content|derivatives)|assignments\/active|companies\/active)$/.test(destination)
        : /^(home|feed|work|requests(?:\/[^/]+)?|handbook(?:\/[^/]+)?|people|settings|inbox|expenses(?:\/[^/]+)?|posts\/[^/]+|staff-tasks(?:\/[^/]+)?)$/.test(destination);
}
