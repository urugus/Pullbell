#import <Cocoa/Cocoa.h>
#import <Foundation/Foundation.h>

@interface PullbellNotificationDelegate : NSObject <NSUserNotificationCenterDelegate>
@end

@implementation PullbellNotificationDelegate

- (BOOL)userNotificationCenter:(NSUserNotificationCenter*)center
     shouldPresentNotification:(NSUserNotification*)notification {
    return YES;
}

- (void)userNotificationCenter:(NSUserNotificationCenter*)center
       didActivateNotification:(NSUserNotification*)notification {
    NSString* urlString = notification.userInfo[@"url"];
    if (urlString.length > 0) {
        NSURL* url = [NSURL URLWithString:urlString];
        if (url != nil) {
            [[NSWorkspace sharedWorkspace] openURL:url];
        }
    }

    [center removeDeliveredNotification:notification];
}

@end

static PullbellNotificationDelegate* pullbellNotificationDelegate = nil;

static NSImage* pullbell_notification_icon(NSString* preferredIconPath) {
    if (preferredIconPath.length > 0) {
        NSImage* preferredIcon = [[NSImage alloc] initWithContentsOfFile:preferredIconPath];
        if (preferredIcon != nil) {
            return preferredIcon;
        }
    }

    NSString* iconPath = [[NSBundle mainBundle] pathForResource:@"Pullbell" ofType:@"icns"];
    if (iconPath.length > 0) {
        return [[NSImage alloc] initWithContentsOfFile:iconPath];
    }

    NSImage* appIcon = [NSApplication sharedApplication].applicationIconImage;
    if (appIcon != nil && appIcon.size.width > 0 && appIcon.size.height > 0) {
        return appIcon;
    }

    return nil;
}

static void pullbell_set_notification_icon(NSUserNotification* notification, NSImage* icon) {
    if (icon == nil) {
        return;
    }

    @try {
        [notification setValue:icon forKey:@"_identityImage"];
        [notification setValue:@NO forKey:@"_identityImageHasBorder"];
    } @catch (NSException* exception) {
        // Private notification icon keys are best-effort and may be unavailable.
    }
}

void pullbell_install_notification_delegate(void) {
    @autoreleasepool {
        if (pullbellNotificationDelegate == nil) {
            pullbellNotificationDelegate = [[PullbellNotificationDelegate alloc] init];
        }

        [NSUserNotificationCenter defaultUserNotificationCenter].delegate =
            pullbellNotificationDelegate;
    }
}

bool pullbell_send_pr_notification(
    const char* title,
    const char* subtitle,
    const char* message,
    const char* url,
    const char* icon_path
) {
    @autoreleasepool {
        pullbell_install_notification_delegate();

        NSString* titleString = [NSString stringWithUTF8String:title];
        NSString* subtitleString = [NSString stringWithUTF8String:subtitle];
        NSString* messageString = [NSString stringWithUTF8String:message];
        NSString* urlString = [NSString stringWithUTF8String:url];
        NSString* iconPathString = [NSString stringWithUTF8String:icon_path];

        if (titleString == nil || messageString == nil || urlString == nil) {
            return false;
        }

        NSUserNotification* notification = [[NSUserNotification alloc] init];
        notification.title = titleString;
        if (subtitleString.length > 0) {
            notification.subtitle = subtitleString;
        }
        notification.informativeText = messageString;
        notification.userInfo = @{@"url": urlString};
        notification.hasActionButton = NO;
        pullbell_set_notification_icon(
            notification,
            pullbell_notification_icon(iconPathString)
        );

        [[NSUserNotificationCenter defaultUserNotificationCenter]
            deliverNotification:notification];

        return true;
    }
}
